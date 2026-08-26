use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use colored::Colorize;

pub const EXIT_LEAK: i32 = 2;

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

fn write_stream(stream: Stream, bytes: &[u8]) -> bool {
    match stream {
        Stream::Out => {
            let out = std::io::stdout();
            let mut lock = out.lock();
            lock.write_all(bytes).and_then(|_| lock.flush()).is_ok()
        }
        Stream::Err => {
            let err = std::io::stderr();
            let mut lock = err.lock();
            lock.write_all(bytes).and_then(|_| lock.flush()).is_ok()
        }
    }
}

fn spawn_pump<R>(reader: R, stream: Stream, needles: Vec<(String, String)>, tx: mpsc::Sender<String>) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    let mut reader = BufReader::new(reader);
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    for (value, label) in &needles {
                        if chunk.contains(value.as_str()) {
                            let _ = tx.send(label.clone());
                        }
                    }
                    if !write_stream(stream, &buffer[..n]) {
                        break;
                    }
                }
            }
        }
    })
}

/// Run `command` with injected env vars. Child stdout/stderr are streamed through
/// untouched, but every chunk is scanned for known secret values. On the first
/// leak the process is terminated immediately and `EXIT_LEAK` is returned.
///
/// `declared_secrets` are the variable names marked `secret: true` in the schema.
/// Undeclared variables whose *name* looks sensitive (KEY/TOKEN/SECRET/PASSWORD)
/// are guarded heuristically as well.
pub fn run(
    command: &[OsString],
    vars: &BTreeMap<String, String>,
    declared_secrets: &[String],
) -> Result<i32> {
    if command.is_empty() {
        anyhow::bail!("nothing to run");
    }

    let program_str = command[0].to_string_lossy().to_string();
    let program = crate::commands::find_in_path(&program_str).unwrap_or_else(|| PathBuf::from(&command[0]));

    let mut cmd = Command::new(program);
    cmd.args(&command[1..])
        .envs(vars)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().with_context(|| format!("spawning `{program_str}`"))?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let needles = build_needles(vars, declared_secrets);

    let (tx, rx) = mpsc::channel::<String>();
    let mut pumps: Vec<JoinHandle<()>> = Vec::new();
    pumps.push(spawn_pump(stdout, Stream::Out, needles.clone(), tx.clone()));
    pumps.push(spawn_pump(stderr, Stream::Err, needles, tx));

    let display = format!(
        "{} {}",
        program_str,
        command[1..]
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );

    let mut hit: Option<String> = None;
    let mut status: Option<std::process::ExitStatus> = None;

    while status.is_none() && hit.is_none() {
        match rx.try_recv() {
            Ok(label) => hit = Some(label),
            Err(TryRecvError::Disconnected) | Err(TryRecvError::Empty) => {}
        }
        if hit.is_some() {
            break;
        }
        match child.try_wait()? {
            Some(done) => status = Some(done),
            None => std::thread::sleep(Duration::from_millis(3)),
        }
    }

    if let Some(leaked_var) = hit {
        let _ = child.kill();
        let _ = child.wait();
        for handle in pumps {
            let _ = handle.join();
        }
        eprintln!();
        eprintln!("{}", "── ENVY GUARD ──────────────────────────────".red().bold());
        eprintln!(
            "{} secret value of {} leaked into process output.",
            "✖".red().bold(),
            leaked_var.red().bold()
        );
        eprintln!("{} process terminated before it could leak more.", "·".dimmed());
        eprintln!("{} fix the offending log/print statement and retry.", "·".dimmed());
        eprintln!("{} {}", "command:".dimmed(), display.dimmed());
        eprintln!("{}", "────────────────────────────────────────────".red().bold());
        return Ok(EXIT_LEAK);
    }

    let done = status.expect("loop exits only with status or hit");
    for handle in pumps {
        let _ = handle.join();
    }
    while rx.try_recv().is_ok() {}

    Ok(match done.code() {
        Some(code) => code,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = done.signal() {
                    return Ok(128 + signal);
                }
            }
            1
        }
    })
}

/// (secret value → variable name) pairs the pump threads scan for.
fn build_needles(
    vars: &BTreeMap<String, String>,
    declared_secrets: &[String],
) -> Vec<(String, String)> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in vars {
        if value.len() < 8 {
            continue;
        }
        let declared = declared_secrets.iter().any(|k| k == key);
        let looks_sensitive = ["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD"]
            .iter()
            .any(|word| key.to_uppercase().contains(word));
        if declared || looks_sensitive {
            map.entry(value.clone()).or_insert_with(|| key.clone());
        }
    }
    map.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn kills_leaking_process() {
        let command: Vec<OsString> = vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo my-super-secret-value-1234".into(),
        ];
        let mut vars = BTreeMap::new();
        vars.insert("API_SECRET".to_string(), "my-super-secret-value-1234".to_string());
        let declared = vec!["API_SECRET".to_string()];
        let code = run(&command, &vars, &declared).expect("runs");
        assert_eq!(code, EXIT_LEAK);
    }

    #[cfg(unix)]
    #[test]
    fn passes_clean_process_through() {
        let command: Vec<OsString> = vec!["/bin/sh".into(), "-c".into(), "echo hello".into()];
        let mut vars = BTreeMap::new();
        vars.insert("API_SECRET".to_string(), "untouched-value-9999".to_string());
        let declared = vec!["API_SECRET".to_string()];
        let code = run(&command, &vars, &declared).expect("runs");
        assert_eq!(code, 0);
    }

    #[test]
    fn short_values_are_ignored() {
        let mut vars = BTreeMap::new();
        vars.insert("SHORT_KEY".to_string(), "abc".to_string());
        assert!(build_needles(&vars, &[]).is_empty());
    }

    #[test]
    fn declared_and_heuristic_names_guarded() {
        let mut vars = BTreeMap::new();
        vars.insert("API_SECRET".to_string(), "declared-value-0001".to_string());
        vars.insert("AUTH_TOKEN".to_string(), "heuristic-value-002".to_string());
        vars.insert("PORT".to_string(), "8080-long-enough-value".to_string());

        let needles = build_needles(&vars, &["API_SECRET".to_string()]);
        let labels: Vec<&str> = needles.iter().map(|(v, _)| v.as_str()).collect();
        assert!(labels.contains(&"declared-value-0001"));
        assert!(labels.contains(&"heuristic-value-002"));
        assert!(!labels.contains(&"8080-long-enough-value"));
    }
}
