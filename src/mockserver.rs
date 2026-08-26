//! Self-mocking HTTP servers: when a variable is declared `mock: true` with
//! `mock_server: true`, envy spins up a local HTTP listener on the fly and
//! points the variable at it — so the app boots even without a real
//! third-party endpoint.

use anyhow::Result;
use colored::Colorize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;

pub struct MockServerPool {
    shutdowns: Vec<mpsc::Sender<()>>,
    handles: Vec<JoinHandle<()>>,
}

impl Drop for MockServerPool {
    fn drop(&mut self) {
        for tx in self.shutdowns.drain(..) {
            let _ = tx.send(());
        }
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

/// Replace `mock_server: true` variables with live local URLs.
/// Returns the pool keeping the servers alive plus the updated values.
pub fn upgrade(
    specs: &[(String, bool)],
    values: &BTreeMap<String, String>,
) -> Result<(MockServerPool, BTreeMap<String, String>)> {
    let mut updated = values.clone();
    let mut shutdowns = Vec::new();
    let mut handles = Vec::new();
    let mut urls = Vec::new();

    for (key, wants_server) in specs {
        if !wants_server || !values.contains_key(key) {
            continue;
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();

        let (tx, rx) = mpsc::channel::<()>();
        let key_clone = key.clone();
        handles.push(std::thread::spawn(move || {
            serve(listener, rx, key_clone);
        }));
        shutdowns.push(tx);

        let url = format!("http://127.0.0.1:{port}");
        updated.insert(key.clone(), url.clone());
        urls.push(format!("{key} → {url}"));
    }

    if !urls.is_empty() {
        println!(
            "{} mocking {} endpoint(s): {}",
            "·".dimmed(),
            urls.len(),
            urls.join(", ")
        );
    }

    Ok((
        MockServerPool { shutdowns, handles },
        updated,
    ))
}

fn serve(listener: TcpListener, shutdown: mpsc::Receiver<()>, key: String) {
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match listener.accept() {
            Ok((stream, _addr)) => handle_connection(stream, key.clone()),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(stream: TcpStream, key: String) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) if line == "\r\n" || line == "\n" || line.trim().is_empty() => break,
            Ok(_) => {}
        }
    }

    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    let body = format!("{{\"mocked\":true,\"key\":\"{key}\",\"path\":\"{path}\"}}\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let mut writer = stream;
    let _ = writer.write_all(response.as_bytes());
}
