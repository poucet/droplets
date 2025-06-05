use std::process::Command;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    // Check if this is a bundle command
    if args.len() > 1 && args[1] == "bundle" {
        // Build frontend first
        build_frontend()?;
    }
    
    // Run the default nih-plug xtask
    nih_plug_xtask::main()
}

fn build_frontend() -> anyhow::Result<()> {
    let frontend_dir = Path::new("frontend");
    
    if !frontend_dir.exists() {
        println!("Frontend directory not found, skipping frontend build");
        return Ok(());
    }
    
    println!("Building frontend resources...");
    
    // Check if node_modules exists, if not run npm install
    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        println!("Installing frontend dependencies...");
        let npm_install = Command::new("npm")
            .arg("install")
            .current_dir(frontend_dir)
            .status()?;
            
        if !npm_install.success() {
            return Err(anyhow::anyhow!("Failed to install frontend dependencies"));
        }
    }
    
    // Build the frontend
    println!("Building frontend with webpack...");
    let npm_build = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(frontend_dir)
        .status()?;
        
    if !npm_build.success() {
        return Err(anyhow::anyhow!("Failed to build frontend"));
    }
    
    println!("Frontend build completed successfully");
    Ok(())
}
