//! Network-level leak guard: a local HTTP proxy the child process is routed
//! through (via HTTP_PROXY/HTTPS_PROXY env). Every outgoing plain-HTTP
//! request line, headers, URL and body are scanned for secret values; a leak
//! is answered with 403 and reported so the process can be terminated.
//!
//! HTTPS: CONNECT tunnels cannot be inspected without MITM certificates.
//! Policy `Block` refuses them outright; policy `Tunnel` passes them through
//! unscanned (documented limitation).

use anyhow::Result;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY_SCAN: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq)]
pub enum TlsPolicy {
    Tunnel,
    Block,
}

pub struct Config {
    /// (secret value, variable name) pairs to scan for.
    pub needles: Vec<(String, String)>,
    pub tls_policy: TlsPolicy,
}

struct Active {
    #[allow(dead_code)]
    port: u16,
    shutdown: mpsc::Sender<()>,
    accept_thread: Option<JoinHandle<()>>,
}

impl Drop for Active {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

pub struct NetGuard {
    #[allow(dead_code)]
    active: Option<Active>,
    pub leaks: Receiver<String>,
    pub port: u16,
}

/// Start the proxy on an ephemeral local port. Drop stops it.
pub fn start(config: Config) -> Result<NetGuard> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let (leak_tx, leak_rx) = mpsc::channel::<String>();

    let cfg = Arc::new(config);
    let accept_thread = std::thread::spawn(move || {
        accept_loop(listener, shutdown_rx, cfg, leak_tx);
    });

    Ok(NetGuard {
        active: Some(Active {
            port,
            shutdown: shutdown_tx,
            accept_thread: Some(accept_thread),
        }),
        leaks: leak_rx,
        port,
    })
}

impl NetGuard {
    /// Poll once for a leaked variable name, if any.
    pub fn try_recv_leak(&self) -> Option<String> {
        self.leaks.try_recv().ok()
    }
}

fn accept_loop(
    listener: TcpListener,
    shutdown: Receiver<()>,
    cfg: Arc<Config>,
    leak_tx: mpsc::Sender<String>,
) {
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let cfg = Arc::clone(&cfg);
                let tx = leak_tx.clone();
                std::thread::spawn(move || handle_connection(stream, cfg, tx));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(stream: TcpStream, cfg: Arc<Config>, leak_tx: mpsc::Sender<String>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut client = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    if debug_net() {
        eprintln!("[envy-net] connection accepted");
    }
    let mut buffered: Vec<u8> = Vec::with_capacity(4096);

    // Read until end of headers (or limits).
    let header_end = loop {
        if let Some(pos) = find_header_end(&buffered) {
            break pos;
        }
        if buffered.len() > MAX_HEAD {
            return;
        }
        let mut chunk = [0u8; 4096];
        match client.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => buffered.extend_from_slice(&chunk[..n]),
        }
    };

    let head_text = String::from_utf8_lossy(&buffered[..header_end]).to_string();
    let request_line = head_text.lines().next().unwrap_or("").to_string();
    if debug_net() {
        eprintln!("[envy-net] request line: {request_line}");
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        return;
    }
    let method = parts[0].to_string();
    let target = parts[1].to_string();

    // CONNECT = TLS tunnel.
    if method == "CONNECT" {
        let host = target.clone();
        let leaked_label = scan_bytes(&buffered[..header_end], &cfg.needles);
        if leaked_label.is_some() || cfg.tls_policy == TlsPolicy::Block {
            let _ = write_response(
                &mut client,
                403,
                "{\"blocked\":true,\"reason\":\"envy net-guard refused tunnel\"}",
            );
            if let Some(label) = leaked_label {
                let _ = leak_tx.send(format!("NET::{label}"));
            } else {
                let _ = leak_tx.send(format!("NET::TLS CONNECT {host}"));
            }
            return;
        }
        let authority = match parse_authority(&target, 443) {
            Some(authority) => authority,
            None => return,
        };
        let _ = write_response(&mut client, 200, "");
        let upstream = match TcpStream::connect((authority.0.as_str(), authority.1)) {
            Ok(upstream) => upstream,
            Err(_) => return,
        };
        pipe_bidirectional(client, upstream);
        return;
    }

    // Plain HTTP: pull in the body when Content-Length is present.
    let content_length = extract_content_length(&head_text);
    let mut total_needed = header_end + content_length.min(MAX_BODY_SCAN);
    while buffered.len() < total_needed.min(buffered.len().saturating_add(MAX_HEAD)) {
        if buffered.len() >= total_needed {
            break;
        }
        let mut chunk = [0u8; 8192];
        match client.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buffered.extend_from_slice(&chunk[..n]),
        }
        total_needed = header_end + content_length.min(MAX_BODY_SCAN);
    }

    if let Some(label) = scan_bytes(&buffered, &cfg.needles) {
        if debug_net() {
            eprintln!("[envy-net] LEAK matched var {label}");
        }
        let _ = write_response(
            &mut client,
            403,
            "{\"blocked\":true,\"reason\":\"secret detected in outbound request\"}",
        );
        let _ = leak_tx.send(format!("NET::{label}"));
        return;
    }

    let Some((host, port)) = parse_authority(&target, 80) else {
        return;
    };
    let Ok(mut upstream) = TcpStream::connect((host.as_str(), port)) else {
        if debug_net() {
            eprintln!("[envy-net] upstream connect failed {host}:{port}");
        }
        let _ = write_response(&mut client, 502, "{\"error\":\"upstream connect failed\"}");
        return;
    };
    if upstream.write_all(&buffered).is_err() {
        return;
    }

    // Client → upstream remainder.
    let mut client_half = match client.try_clone() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut upstream_half = match upstream.try_clone() {
        Ok(u) => u,
        Err(_) => return,
    };
    let up_pump = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match client_half.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if upstream_half.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = upstream_half.shutdown(std::net::Shutdown::Write);
    });

    // Upstream → client.
    let mut buf = [0u8; 8192];
    loop {
        match upstream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if client.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = up_pump.join();
}

fn pipe_bidirectional(a: TcpStream, b: TcpStream) {
    let mut a_read = match a.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut b_write = match b.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut b_read = b;
    let mut a_write = a;

    let t = std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        loop {
            match a_read.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if b_write.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = b_write.shutdown(std::net::Shutdown::Write);
    });

    let mut buf = [0u8; 16384];
    loop {
        match b_read.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if a_write.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = a_write.shutdown(std::net::Shutdown::Write);
    let _ = t.join();
}

fn debug_net() -> bool {
    std::env::var_os("ENVY_DEBUG_NET").is_some()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| pos + 4)
        .or_else(|| {
            buf.windows(2)
                .position(|window| window == b"\n\n")
                .map(|pos| pos + 2)
        })
}

fn scan_bytes(bytes: &[u8], needles: &[(String, String)]) -> Option<String> {
    if needles.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    for (value, label) in needles {
        if value.len() >= 8 && text.contains(value.as_str()) {
            return Some(label.clone());
        }
    }
    None
}

fn extract_content_length(head_text: &str) -> usize {
    for line in head_text.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

/// Parse `http://host[:port]/path`, `host[:port]/path` or `host:port`
/// into `(host, port)` with scheme-default ports.
fn parse_authority(target: &str, default_port: u16) -> Option<(String, u16)> {
    let rest = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .unwrap_or(target);
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    if let Some(host_port) = authority.rsplit_once(':') {
        let host = host_port.0.to_string();
        let port = host_port.1.parse::<u16>().ok()?;
        return Some((host, port));
    }
    Some((authority.to_string(), default_port))
}

fn write_response(stream: &mut TcpStream, status: u16, json_body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "Connection established",
        403 => "Forbidden",
        502 => "Bad Gateway",
        _ => "OK",
    };
    let body_bytes = json_body.as_bytes();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.write_all(body_bytes)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_header_end_crlf() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        assert_eq!(find_header_end(buf), Some(buf.len() - 4));
    }

    #[test]
    fn finds_header_end_lf_only() {
        let buf = b"GET / HTTP/1.1\nHost: x\n\nbody";
        assert_eq!(find_header_end(buf), Some(buf.len() - 4));
    }

    #[test]
    fn parses_authorities() {
        assert_eq!(
            parse_authority("http://example.com/path?q=1", 80),
            Some(("example.com".to_string(), 80))
        );
        assert_eq!(
            parse_authority("http://api.local:9090/x", 80),
            Some(("api.local".to_string(), 9090))
        );
        assert_eq!(parse_authority("db.internal:5432", 443), Some(("db.internal".to_string(), 5432)));
        assert_eq!(parse_authority("", 80), None);
    }

    #[test]
    fn scans_needles_in_bytes() {
        let needles = vec![("super-secret-value-9876".to_string(), "API_SECRET".to_string())];
        assert_eq!(
            scan_bytes(b"GET /?k=super-secret-value-9876 HTTP/1.1", &needles),
            Some("API_SECRET".to_string())
        );
        assert_eq!(scan_bytes(b"GET /clean", &needles), None);
    }

    #[test]
    fn content_length_extraction() {
        let head = "POST /x HTTP/1.1\r\nContent-Length: 42\r\nHost: h\r\n";
        assert_eq!(extract_content_length(head), 42);
        assert_eq!(extract_content_length("GET / HTTP/1.1\r\n"), 0);
    }
}
