//! CopperMoon CLI
//!
//! The main entry point for the CopperMoon runtime.

mod cli;
mod repl;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use colored::Colorize;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    // Setup tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run { file, args }) => {
            run_file(&file, args)?;
        }
        Some(Commands::Repl) => {
            repl::start()?;
        }
        Some(Commands::Version) => {
            print_version();
        }
        None => {
            // If a file is provided as first argument, run it
            if let Some(file) = cli.file {
                run_file(&file, cli.args)?;
            } else {
                // Otherwise, start REPL
                repl::start()?;
            }
        }
    }

    Ok(())
}

fn run_file(file: &str, args: Vec<String>) -> Result<()> {
    let path = std::path::Path::new(file);

    // Canonicalize the path to get absolute path
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    let base_path = absolute_path.parent().unwrap_or(std::path::Path::new("."));
    let file_name = absolute_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file);

    let runtime = coppermoon_core::Runtime::with_base_path(base_path)?;

    // Setup module loader
    runtime.setup_module_loader()?;

    // Register standard library
    coppermoon_std::register_all(runtime.lua())?;

    // Register SQLite module
    coppermoon_sqlite::register_global(runtime.lua())?;

    // Register MySQL module
    coppermoon_mysql::register_global(runtime.lua())?;

    // Set script arguments
    let lua = runtime.lua();
    let arg_table = lua.create_table()?;

    // arg[0] is the script name (original path given by user)
    arg_table.set(0, file)?;

    // arg[1..] are the arguments
    for (i, arg) in args.iter().enumerate() {
        arg_table.set(i as i64 + 1, arg.as_str())?;
    }

    lua.globals().set("arg", arg_table)?;

    // Execute the file (just the filename, base_path is already set)
    if let Err(e) = runtime.exec_file(file_name) {
        eprintln!("{}: {}", "error".red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}

fn print_version() {
    println!(
        "{} {}",
        "CopperMoon".bright_yellow().bold(),
        env!("CARGO_PKG_VERSION")
    );
    println!("Lua 5.4 runtime written in Rust");
}
