//! Interactive REPL for CopperMoon

use anyhow::Result;
use colored::Colorize;
use coppermoon_core::Runtime;
use std::io::{self, BufRead, Write};

/// Start the interactive REPL
pub fn start() -> Result<()> {
    println!(
        "{} {} - Interactive Mode",
        "CopperMoon".bright_yellow().bold(),
        env!("CARGO_PKG_VERSION")
    );
    println!("Type {} to exit, {} for help", ".exit".cyan(), ".help".cyan());
    println!();

    let runtime = Runtime::new()?;
    runtime.setup_module_loader()?;
    coppermoon_std::register_all(runtime.lua())?;
    coppermoon_sqlite::register_global(runtime.lua())?;
    coppermoon_mysql::register_global(runtime.lua())?;
    coppermoon_postgresql::register_global(runtime.lua())?;

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Print prompt
        print!("{} ", ">".bright_green().bold());
        stdout.flush()?;

        // Read input
        let mut input = String::new();
        let bytes_read = stdin.lock().read_line(&mut input)?;

        // Handle EOF (Ctrl+D)
        if bytes_read == 0 {
            println!();
            break;
        }

        let input = input.trim();

        // Skip empty lines
        if input.is_empty() {
            continue;
        }

        // Handle REPL commands
        if input.starts_with('.') {
            match input {
                ".exit" | ".quit" | ".q" => break,
                ".help" | ".h" => {
                    print_help();
                    continue;
                }
                ".clear" | ".cls" => {
                    // Clear screen (ANSI escape code)
                    print!("\x1B[2J\x1B[1;1H");
                    stdout.flush()?;
                    continue;
                }
                _ => {
                    eprintln!("{}: Unknown command '{}'", "error".red(), input);
                    continue;
                }
            }
        }

        // Handle multi-line input (incomplete statements)
        let mut code = input.to_string();
        while is_incomplete(&code) {
            print!("{} ", "..".bright_black());
            stdout.flush()?;

            let mut continuation = String::new();
            if stdin.lock().read_line(&mut continuation)? == 0 {
                break;
            }
            code.push('\n');
            code.push_str(&continuation);
        }

        // Try to evaluate as expression first (for REPL convenience)
        let eval_code = if !code.starts_with("return ")
            && !code.contains('=')
            && !code.starts_with("local ")
            && !code.starts_with("function ")
            && !code.starts_with("if ")
            && !code.starts_with("for ")
            && !code.starts_with("while ")
            && !code.starts_with("repeat ")
        {
            format!("return {}", code)
        } else {
            code.clone()
        };

        // Try eval first, then exec
        match runtime.eval(&eval_code) {
            Ok(result) if !result.is_empty() && result != "nil" => {
                println!("{}", result.bright_white());
            }
            Ok(_) => {
                // Expression returned nil, try as statement
                if let Err(e) = runtime.exec(&code) {
                    print_error(&e.to_string());
                }
            }
            Err(_) => {
                // Eval failed, try as statement
                if let Err(e) = runtime.exec(&code) {
                    print_error(&e.to_string());
                }
            }
        }
    }

    println!("{}", "Goodbye!".bright_yellow());
    Ok(())
}

/// Check if the code is incomplete (needs more input)
fn is_incomplete(code: &str) -> bool {
    let code = code.trim();

    // Simple heuristics for incomplete statements
    let opens = code.matches("function").count()
        + code.matches("if").count()
        + code.matches("for").count()
        + code.matches("while").count()
        + code.matches("repeat").count()
        + code.matches("do").count();

    let closes = code.matches("end").count() + code.matches("until").count();

    opens > closes
}

fn print_help() {
    println!("{}", "REPL Commands:".bright_yellow().bold());
    println!("  {}  - Exit the REPL", ".exit".cyan());
    println!("  {}  - Show this help", ".help".cyan());
    println!("  {} - Clear the screen", ".clear".cyan());
    println!();
    println!("{}", "Tips:".bright_yellow().bold());
    println!("  - Expressions are automatically printed");
    println!("  - Multi-line input is supported");
    println!("  - Press Ctrl+D to exit");
}

fn print_error(msg: &str) {
    // Clean up the error message
    let msg = msg
        .replace("[string \"??\"]:", "")
        .replace("runtime error: ", "");
    eprintln!("{}: {}", "error".red().bold(), msg);
}
