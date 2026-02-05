//! CLI argument parsing

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coppermoon")]
#[command(author, version, about = "A high-performance Lua runtime written in Rust")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Lua file to execute (shorthand for `coppermoon run <file>`)
    pub file: Option<String>,

    /// Arguments to pass to the Lua script
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a Lua file
    Run {
        /// The Lua file to execute
        file: String,

        /// Arguments to pass to the script
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start the interactive REPL
    Repl,

    /// Show version information
    Version,
}
