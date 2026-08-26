use crate::resolver::scalar_to_string;
use crate::schema::VarSpec;
use colored::Colorize;
use std::io::{self, BufRead, IsTerminal, Write};

pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

/// Ask the user for a value interactively. Returns `None` on empty input (skip).
pub fn ask(key: &str, spec: &VarSpec) -> io::Result<Option<String>> {
    print!("\n{} {}", "?".cyan().bold(), key.bold());

    if let Some(description) = &spec.description {
        print!(" {}", format!("({description})").dimmed());
    }

    let type_hint = match spec.r#type.as_str() {
        "integer" => "integer",
        "number" | "float" => "number",
        "boolean" | "bool" => "boolean",
        _ => "string",
    };
    print!(" {}", format!("[{type_hint}]").dimmed());

    if let Some(default) = spec.default.as_ref().and_then(|d| scalar_to_string(d).ok()) {
        print!(" {}", format!("[default: {default}]").dimmed());
    }

    if spec.secret {
        print!(" {}", "(secret)".yellow());
    }

    print!(": ");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim();

    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}
