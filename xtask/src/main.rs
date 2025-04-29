use std::env;
use std::error::Error;
use std::process::{self, Command};

fn main() {
    if let Err(err) = try_main() {
        eprintln!("ERROR: {}", err);
        process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("bundle") => {
            // Get the arguments to pass to the bundler
            let args: Vec<String> = env::args().skip(2).collect();
            
            // Use cargo run to execute the bundle command
            let status = Command::new("cargo")
                .arg("run")
                .arg("--package=nih_plug_xtask")
                .arg("--")
                .arg("bundle")
                .arg("granular")
                .args(&args)
                .status()?;
                
            if !status.success() {
                return Err("Bundle command failed".into());
            }
            
            println!("Plugin bundled successfully!");
            Ok(())
        }
        _ => {
            // Print usage information if no valid task is provided
            println!("Usage: cargo xtask <task>");
            println!();
            println!("Available tasks:");
            println!("  bundle [--release] - Bundle the plugin");
            Ok(())
        }
    }
}
