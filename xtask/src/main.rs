use std::env;
use std::fs;
use std::process::Command;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Build automation for Simply Droplets plugin")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build and bundle the plugin
    Build {
        /// Build profile to use
        #[arg(long, short, default_value = "release")]
        profile: Profile,
        
        /// Plugin format to build
        #[arg(long, short, default_value = "both")]
        format: Format,
    },
}

#[derive(ValueEnum, Clone)]
enum Profile {
    Debug,
    Release,
}

impl Profile {
    fn as_str(&self) -> &'static str {
        match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    }
}

#[derive(ValueEnum, Clone)]
enum Format {
    Clap,
    Vst3,
    Both,
}


fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Build { profile, format } => {
            let profile_str = profile.as_str();
            
            // Build the React frontend first
            build_frontend()?;
            
            // Then, build the plugin
            build_plugin(profile_str)?;
            
            // Finally, create the bundle(s)
            match format {
                Format::Clap => create_clap_bundle(profile_str)?,
                Format::Vst3 => create_vst3_bundle(profile_str)?,
                Format::Both => {
                    create_clap_bundle(profile_str)?;
                    create_vst3_bundle(profile_str)?;
                }
            }
        }
    }
    
    Ok(())
}

fn build_frontend() -> anyhow::Result<()> {
    // Determine project root - if we're in xtask directory, go up one level
    let current_dir = env::current_dir()?;
    let project_root = if current_dir.file_name().and_then(|n| n.to_str()) == Some("xtask") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    };
    let frontend_dir = project_root.join("frontend");
    
    if !frontend_dir.exists() {
        println!("Frontend directory not found, skipping frontend build");
        return Ok(());
    }
    
    println!("Building React frontend...");
    
    // Check if node_modules exists, if not run npm install
    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        println!("Installing frontend dependencies...");
        let npm_install = Command::new("npm")
            .arg("install")
            .current_dir(&frontend_dir)
            .status()?;
            
        if !npm_install.success() {
            return Err(anyhow::anyhow!("Failed to install frontend dependencies"));
        }
    }
    
    // Build the frontend
    println!("Building React bundle with webpack...");
    let npm_build = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(&frontend_dir)
        .status()?;
        
    if !npm_build.success() {
        return Err(anyhow::anyhow!("Failed to build React frontend"));
    }
    
    println!("React frontend build completed successfully");
    Ok(())
}

fn build_plugin(profile: &str) -> anyhow::Result<()> {
    println!("Building plugin...");
    
    // Determine project root - if we're in xtask directory, go up one level
    let current_dir = env::current_dir()?;
    let project_root = if current_dir.file_name().and_then(|n| n.to_str()) == Some("xtask") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    };
    
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .current_dir(&project_root);
    
    if profile == "release" {
        cmd.arg("--release");
    }
    
    // Check for VST3 SDK environment variable
    if let Ok(vst3_sdk_path) = env::var("CLAP_WRAPPER_VST3_SDK") {
        println!("Using VST3 SDK at: {}", vst3_sdk_path);
        cmd.env("CLAP_WRAPPER_VST3_SDK", vst3_sdk_path);
    } else {
        println!("Warning: CLAP_WRAPPER_VST3_SDK not set. VST3 export may not work properly.");
        println!("Please set the environment variable to point to your VST3 SDK installation.");
    }
    
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("Failed to build plugin"));
    }
    
    Ok(())
}

fn create_clap_bundle(profile: &str) -> anyhow::Result<()> {
    // Determine project root - if we're in xtask directory, go up one level
    let current_dir = env::current_dir()?;
    let project_root = if current_dir.file_name().and_then(|n| n.to_str()) == Some("xtask") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    };
    let target_dir = project_root.join("target");
    let profile_dir = target_dir.join(profile);
    let bundle_dir = target_dir.join("bundle");
    let bundle_name = "Simply Droplets.clap";
    let bundle_path = bundle_dir.join(bundle_name);
    let contents_path = bundle_path.join("Contents");
    let macos_path = contents_path.join("MacOS");
    
    // Check if the dylib exists
    let dylib_src = profile_dir.join("libsimply_droplets.dylib");
    if !dylib_src.exists() {
        return Err(anyhow::anyhow!(
            "libsimply_droplets.dylib not found at {:?}. Please run 'cargo build{}' first",
            dylib_src,
            if profile == "release" { " --release" } else { "" }
        ));
    }
    
    // Create bundle directory structure
    if bundle_path.exists() {
        fs::remove_dir_all(&bundle_path)?;
    }
    fs::create_dir_all(&macos_path)?;
    
    // Copy the dylib to the MacOS folder with the correct name for CLAP
    let dylib_dst = macos_path.join("Simply Droplets");
    fs::copy(&dylib_src, &dylib_dst)?;
    
    // Fix dynamic library paths to make the bundle self-contained
    fix_dylib_paths(&dylib_dst, &dylib_src)?;
    
    // Create a basic Info.plist for CLAP
    let info_plist_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>Simply Droplets</string>
    <key>CFBundleIconFile</key>
    <string></string>
    <key>CFBundleIdentifier</key>
    <string>com.simply-chris.simply-droplets</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Simply Droplets</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
</dict>
</plist>"#;
    
    let plist_dst = contents_path.join("Info.plist");
    fs::write(&plist_dst, info_plist_content)?;
    
    // Create PkgInfo file
    let pkginfo_dst = contents_path.join("PkgInfo");
    fs::write(&pkginfo_dst, "BNDL????")?;
    
    // Apply ad-hoc code signature
    let codesign_status = Command::new("codesign")
        .args(["-s", "-", bundle_path.to_str().unwrap()])
        .status();
    
    match codesign_status {
        Ok(status) if !status.success() => {
            println!("Warning: Failed to sign CLAP bundle");
        }
        Err(_) => {
            println!("Warning: codesign not available, bundle not signed");
        }
        _ => {}
    }
    
    println!("Created CLAP bundle at: {}", bundle_path.display());
    Ok(())
}

fn create_vst3_bundle(profile: &str) -> anyhow::Result<()> {
    // Determine project root - if we're in xtask directory, go up one level
    let current_dir = env::current_dir()?;
    let project_root = if current_dir.file_name().and_then(|n| n.to_str()) == Some("xtask") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    };
    let target_dir = project_root.join("target");
    let profile_dir = target_dir.join(profile);
    let bundle_dir = target_dir.join("bundle");
    let bundle_name = "Simply Droplets.vst3";
    let bundle_path = bundle_dir.join(bundle_name);
    let contents_path = bundle_path.join("Contents");
    let macos_path = contents_path.join("MacOS");
    
    // Check if the dylib exists
    let dylib_src = profile_dir.join("libsimply_droplets.dylib");
    if !dylib_src.exists() {
        return Err(anyhow::anyhow!(
            "libsimply_droplets.dylib not found at {:?}. Please run 'cargo build{}' first",
            dylib_src,
            if profile == "release" { " --release" } else { "" }
        ));
    }
    
    // Create bundle directory structure
    if bundle_path.exists() {
        fs::remove_dir_all(&bundle_path)?;
    }
    fs::create_dir_all(&macos_path)?;
    
    // Copy the dylib to the MacOS folder with the correct name for VST3
    let dylib_dst = macos_path.join("Simply Droplets");
    fs::copy(&dylib_src, &dylib_dst)?;
    
    // Fix dynamic library paths to make the bundle self-contained
    fix_dylib_paths(&dylib_dst, &dylib_src)?;
    
    // Create a basic Info.plist for VST3
    let info_plist_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>Simply Droplets</string>
    <key>CFBundleIconFile</key>
    <string></string>
    <key>CFBundleIdentifier</key>
    <string>com.simply-chris.simply-droplets.vst3</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Simply Droplets</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
</dict>
</plist>"#;
    
    let plist_dst = contents_path.join("Info.plist");
    fs::write(&plist_dst, info_plist_content)?;
    
    // Create PkgInfo file
    let pkginfo_dst = contents_path.join("PkgInfo");
    fs::write(&pkginfo_dst, "BNDL????")?;
    
    // Apply ad-hoc code signature
    let codesign_status = Command::new("codesign")
        .args(["-s", "-", bundle_path.to_str().unwrap()])
        .status();
    
    match codesign_status {
        Ok(status) if !status.success() => {
            println!("Warning: Failed to sign VST3 bundle");
        }
        Err(_) => {
            println!("Warning: codesign not available, bundle not signed");
        }
        _ => {}
    }
    
    println!("Created VST3 bundle at: {}", bundle_path.display());
    Ok(())
}

fn fix_dylib_paths(dylib_path: &std::path::Path, original_dylib: &std::path::Path) -> anyhow::Result<()> {
    use std::path::Path;
    
    // Change the install name to use @loader_path (relative to the bundle executable)
    let new_id = format!("@loader_path/{}", dylib_path.file_name().unwrap().to_str().unwrap());
    
    println!("Fixing dylib install name to: {}", new_id);
    
    let install_name_status = Command::new("install_name_tool")
        .args(["-id", &new_id, dylib_path.to_str().unwrap()])
        .status();
    
    match install_name_status {
        Ok(status) if status.success() => {
            println!("Successfully updated dylib install name");
        }
        Ok(_) => {
            println!("Warning: Failed to update dylib install name");
        }
        Err(e) => {
            println!("Warning: install_name_tool not available: {}", e);
        }
    }
    
    // Also fix any dependencies that might point to the build directory
    let original_path = original_dylib.to_str().unwrap();
    let change_status = Command::new("install_name_tool")
        .args(["-change", original_path, &new_id, dylib_path.to_str().unwrap()])
        .status();
    
    match change_status {
        Ok(status) if status.success() => {
            println!("Successfully updated dylib dependency paths");
        }
        Ok(_) => {
            // This is expected to fail if there are no matching dependencies
        }
        Err(e) => {
            println!("Warning: install_name_tool not available: {}", e);
        }
    }
    
    Ok(())
}
