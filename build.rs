use std::process::Command;
use std::path::Path;
use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/package.json");
    
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