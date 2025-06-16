use std::process::Command;
use std::path::Path;
use std::fs;
use std::env;

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/dist/bundle.js");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("react_bundle.rs");
    
    // Build the frontend if we're in development
    let frontend_dir = Path::new("frontend");
    if frontend_dir.exists() {
        println!("Building React frontend...");
        
        // Change to frontend directory and run build
        let output = Command::new("npm")
            .args(&["run", "build"])
            .current_dir(frontend_dir)
            .output();
            
        match output {
            Ok(output) => {
                if !output.status.success() {
                    println!("cargo:warning=Frontend build failed: {}", 
                        String::from_utf8_lossy(&output.stderr));
                } else {
                    println!("Frontend build successful");
                    
                    // Copy dist to target directory for bundling
                    let src = frontend_dir.join("dist");
                    let dest = Path::new("target").join("frontend-dist");
                    
                    if src.exists() {
                        let _ = fs::remove_dir_all(&dest);
                        copy_dir_recursive(&src, &dest);
                        println!("Frontend copied to target/frontend-dist");
                    }
                }
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm build: {}", e);
            }
        }
    }
    
    // Generate React bundle constant
    let bundle_path = "frontend/dist/bundle.js";
    if Path::new(bundle_path).exists() {
        match fs::read_to_string(bundle_path) {
            Ok(bundle_content) => {
                // Generate a Rust constant with the bundle content
                let generated_code = format!(
                    "pub const REACT_BUNDLE: &str = r#\"{}\"#;",
                    bundle_content.replace("\\", "\\\\").replace("\"", "\\\"")
                );
                
                if let Err(e) = fs::write(&dest_path, generated_code) {
                    println!("cargo:warning=Failed to write react_bundle.rs: {}", e);
                }
            }
            Err(e) => {
                println!("cargo:warning=Failed to read React bundle: {}", e);
                // Create empty bundle constant
                let generated_code = "pub const REACT_BUNDLE: &str = \"\";";
                let _ = fs::write(&dest_path, generated_code);
            }
        }
    } else {
        // Create empty bundle constant
        let generated_code = "pub const REACT_BUNDLE: &str = \"\";";
        let _ = fs::write(&dest_path, generated_code);
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) {
    if let Err(e) = fs::create_dir_all(dest) {
        println!("cargo:warning=Failed to create dest dir: {}", e);
        return;
    }
    
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let src_path = entry.path();
            let dest_path = dest.join(entry.file_name());
            
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dest_path);
            } else if let Err(e) = fs::copy(&src_path, &dest_path) {
                println!("cargo:warning=Failed to copy {}: {}", src_path.display(), e);
            }
        }
    }
}