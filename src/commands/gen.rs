use super::load_app;
use envy::gencode::{self, Target};
use anyhow::{Context, Result};
use colored::Colorize;

pub struct GenArgs {
    pub target: Target,
    pub out: Option<std::path::PathBuf>,
    pub package: String,
}

pub fn execute(args: GenArgs) -> Result<()> {
    let app = load_app()?;
    let schema = &app.project.schema;

    let code = gencode::generate(&args.target, schema, &args.package);
    let out_path = args.out.unwrap_or_else(|| {
        app.project.dir().join(args.target.default_filename())
    });

    std::fs::write(&out_path, &code)
        .with_context(|| format!("writing {}", out_path.display()))?;

    println!(
        "{} generated type-safe bindings for {} variable(s) → {}",
        "✔".green().bold(),
        schema.config.len(),
        out_path.display().to_string().cyan()
    );
    match args.target {
        Target::TypeScript => println!(
            "{} reference it with {} or include it via tsconfig {} array",
            "·".dimmed(),
            "/// <reference path=\"envy.d.ts\" />".cyan(),
            "\"files\"".cyan()
        ),
        Target::Go => println!("{} load with config.Load() in your main()", "·".dimmed()),
        Target::Java => println!("{} inject EnvyConfig anywhere with @Autowired", "·".dimmed()),
        Target::Python => println!("{} import it: from envy_config import *", "·".dimmed()),
    }
    Ok(())
}
