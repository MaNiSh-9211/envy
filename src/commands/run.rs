use super::{
    abort_if_broken, find_in_path, interactive, load_app, report_problems,
};
use crate::guard;
use crate::netguard::{self, Config as NetConfig, TlsPolicy};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

pub struct RunArgs {
    pub guard: bool,
    pub guard_net: bool,
    pub allow_tls: bool,
    pub command: Vec<OsString>,
}

pub fn execute(args: RunArgs, offline: bool) -> Result<i32> {
    if args.command.is_empty() {
        bail!("nothing to run — usage: envy run [--guard] [--guard-net] <command> [args...]");
    }

    let app = load_app()?;
    let opts = envy::resolver::Options {
        interactive: interactive(),
        resolve_vault: !offline,
    };
    let resolved = app.resolve(&opts);

    super::persist_prompted(&app, &resolved)?;
    report_problems(&resolved);
    super::note_schema_drift(&app);
    if abort_if_broken(&resolved) {
        return Ok(1);
    }

    let mut vars = resolved.values.clone();

    // Self-mocking HTTP endpoints for mock + mock_server variables.
    let mock_specs: Vec<(String, bool)> = app
        .project
        .schema
        .config
        .iter()
        .filter(|(_, spec)| spec.mock && spec.mock_server)
        .map(|(key, _)| (key.clone(), true))
        .collect();
    let (_mock_pool, values_after_mocks) = crate::mockserver::upgrade(&mock_specs, &vars)?;
    vars = values_after_mocks;

    let secret_names: Vec<String> = app
        .project
        .schema
        .config
        .iter()
        .filter(|(_, spec)| spec.secret)
        .map(|(key, _)| key.clone())
        .collect();

    // Network-level leak guard: local proxy routed via HTTP(S)_PROXY env.
    let mut net = if args.guard_net {
        let needles = guard::build_needles(&vars, &secret_names);
        let tls_policy = if args.allow_tls {
            TlsPolicy::Tunnel
        } else {
            TlsPolicy::Block
        };
        Some(netguard::start(NetConfig {
            needles,
            tls_policy,
        })?)
    } else {
        None
    };

    if let Some(net_guard) = net.as_ref() {
        let proxy_addr = format!("http://127.0.0.1:{}", net_guard.port);
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            vars.insert(key.to_string(), proxy_addr.clone());
        }
        for key in ["NO_PROXY", "no_proxy"] {
            vars.insert(key.to_string(), "localhost,127.0.0.1,::1".to_string());
        }
        println!(
            "{} net-guard proxy on {} (TLS {}) — outbound requests are scanned for secrets",
            "·".dimmed(),
            proxy_addr.cyan(),
            if args.allow_tls { "tunneled" } else { "blocked" }
        );
    }

    let display = args
        .command
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let mut mode_bits = Vec::new();
    if args.guard {
        mode_bits.push("guard on");
    }
    if net.is_some() {
        mode_bits.push("net-guard on");
    }
    let mode = if mode_bits.is_empty() {
        String::new()
    } else {
        format!(" ({})", mode_bits.join(", "))
    };
    println!(
        "{} injecting {} variable(s) into `{}`{mode}",
        "·".dimmed(),
        vars.len(),
        display.cyan()
    );

    if let Some(branch) = &app.branch {
        if app.overlay_path.is_some() {
            println!("{} branch overlay active: {}", "·".dimmed(), branch.cyan());
        }
    }

    let code = if args.guard || net.is_some() {
        guard::run(&args.command, &vars, &secret_names, net.as_mut())?
    } else {
        launch(&args.command, &vars)?
    };

    drop(net);
    Ok(code)
}

fn launch(command: &[OsString], vars: &BTreeMap<String, String>) -> Result<i32> {
    use std::process::Command;

    let program_str = command[0].to_string_lossy().to_string();
    let program = find_in_path(&program_str).unwrap_or_else(|| PathBuf::from(&command[0]));

    let mut cmd = Command::new(program);
    cmd.args(&command[1..]);
    for (key, value) in vars {
        cmd.env(key, value);
    }

    let status = cmd.status().with_context(|| format!("spawning `{program_str}`"))?;

    Ok(match status.code() {
        Some(code) => code,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    return Ok(128 + signal);
                }
            }
            1
        }
    })
}
