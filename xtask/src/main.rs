use std::env;
use std::fs;
use std::path::{Path, PathBuf};
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
    /// Build and bundle the plugin for the current platform
    Build {
        /// Build profile to use
        #[arg(long, short, default_value = "release")]
        profile: Profile,

        /// Plugin format to build
        #[arg(long, short, default_value = "both")]
        format: Format,

        /// Enable development GUI features (devtools, debugging)
        #[arg(long)]
        dev_gui: bool,
    },
    /// Install bundled plugin(s) into the current user's DAW plugin folders.
    /// Runs `build` first unless --skip-build is passed.
    Install {
        /// Skip the build step and install whatever is already in target/bundle/
        #[arg(long)]
        skip_build: bool,

        /// Build profile (ignored with --skip-build)
        #[arg(long, short, default_value = "release")]
        profile: Profile,

        /// Plugin format (ignored with --skip-build)
        #[arg(long, short, default_value = "both")]
        format: Format,

        /// Enable development GUI features (ignored with --skip-build)
        #[arg(long)]
        dev_gui: bool,
    },
    /// Generate TypeScript types from Rust structs
    GenTypes,
}

#[derive(ValueEnum, Clone, Copy)]
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

#[derive(ValueEnum, Clone, Copy, PartialEq, Eq)]
enum Format {
    Clap,
    Vst3,
    Both,
}

impl Format {
    fn wants_clap(self) -> bool {
        matches!(self, Format::Clap | Format::Both)
    }
    fn wants_vst3(self) -> bool {
        matches!(self, Format::Vst3 | Format::Both)
    }
}

// ---------------------------------------------------------------------------
// Host platform abstraction — keeps build/install symmetric across OSes.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum HostOs {
    Macos,
    Linux,
    Windows,
}

impl HostOs {
    fn detect() -> anyhow::Result<Self> {
        match env::consts::OS {
            "macos" => Ok(HostOs::Macos),
            "linux" => Ok(HostOs::Linux),
            "windows" => Ok(HostOs::Windows),
            other => Err(anyhow::anyhow!("Unsupported host OS: {}", other)),
        }
    }

    /// File name cargo emits for the cdylib on this OS, e.g. `libsimply_droplets.dylib`.
    fn dylib_filename(self) -> &'static str {
        match self {
            HostOs::Macos => "libsimply_droplets.dylib",
            HostOs::Linux => "libsimply_droplets.so",
            HostOs::Windows => "simply_droplets.dll",
        }
    }

    /// VST3 subdir name inside the bundle's `Contents/` dir (per VST3 spec).
    fn vst3_arch_subdir(self) -> &'static str {
        // Simple arch detection; users cross-compiling should set this manually.
        match (self, env::consts::ARCH) {
            (HostOs::Macos, _) => "MacOS",
            (HostOs::Linux, "x86_64") => "x86_64-linux",
            (HostOs::Linux, "aarch64") => "aarch64-linux",
            (HostOs::Windows, "x86_64") => "x86_64-win",
            (HostOs::Windows, "aarch64") => "aarch64-win",
            // Fallback — VST3 hosts will ignore unknown subdirs but we still
            // produce something rather than failing outright.
            (HostOs::Linux, arch) => Box::leak(format!("{}-linux", arch).into_boxed_str()),
            (HostOs::Windows, arch) => Box::leak(format!("{}-win", arch).into_boxed_str()),
        }
    }

    /// User-scope install directories for {CLAP, VST3} plugins.
    /// Returns (clap_dir, vst3_dir).
    fn user_plugin_dirs(self) -> anyhow::Result<(PathBuf, PathBuf)> {
        match self {
            HostOs::Macos => {
                let home = env::var("HOME").map_err(|_| anyhow::anyhow!("HOME not set"))?;
                let base = PathBuf::from(home).join("Library/Audio/Plug-Ins");
                Ok((base.join("CLAP"), base.join("VST3")))
            }
            HostOs::Linux => {
                let home = env::var("HOME").map_err(|_| anyhow::anyhow!("HOME not set"))?;
                let base = PathBuf::from(home);
                Ok((base.join(".clap"), base.join(".vst3")))
            }
            HostOs::Windows => {
                // User-scope on Windows is %LOCALAPPDATA%\Programs\Common\{CLAP,VST3}
                // per the respective specs; fall back to %APPDATA% if unset.
                let appdata = env::var("LOCALAPPDATA")
                    .or_else(|_| env::var("APPDATA"))
                    .map_err(|_| anyhow::anyhow!("LOCALAPPDATA/APPDATA not set"))?;
                let base = PathBuf::from(appdata).join("Programs").join("Common");
                Ok((base.join("CLAP"), base.join("VST3")))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BundleInfo — single source of truth for plugin display name, version, and
// bundle identifiers. All values come from Cargo.toml (standard `package`
// fields + a `[package.metadata.bundle]` table) via `cargo metadata`, so
// there's no separate bundler.toml to keep in sync.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct BundleInfo {
    display_name: String,
    version: String,
    clap_bundle_id: String,
    vst3_bundle_id: String,
}

impl BundleInfo {
    fn load(project_root: &Path) -> anyhow::Result<Self> {
        let output = Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(project_root)
            .output()?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        #[derive(serde::Deserialize)]
        struct Metadata {
            packages: Vec<Package>,
        }
        #[derive(serde::Deserialize)]
        struct Package {
            name: String,
            version: String,
            metadata: Option<serde_json::Value>,
        }

        let meta: Metadata = serde_json::from_slice(&output.stdout)?;
        let pkg = meta
            .packages
            .into_iter()
            .find(|p| p.name == "simply_droplets")
            .ok_or_else(|| anyhow::anyhow!("simply_droplets package not found in metadata"))?;

        let bundle = pkg
            .metadata
            .as_ref()
            .and_then(|m| m.get("bundle"))
            .ok_or_else(|| {
                anyhow::anyhow!("[package.metadata.bundle] missing from Cargo.toml")
            })?;

        let get_str = |key: &str| -> anyhow::Result<String> {
            bundle
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!("[package.metadata.bundle].{} missing or non-string", key)
                })
        };

        Ok(BundleInfo {
            display_name: get_str("display_name")?,
            version: pkg.version,
            clap_bundle_id: get_str("clap_bundle_id")?,
            vst3_bundle_id: get_str("vst3_bundle_id")?,
        })
    }

    fn clap_bundle_name(&self) -> String {
        format!("{}.clap", self.display_name)
    }
    fn vst3_bundle_name(&self) -> String {
        format!("{}.vst3", self.display_name)
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { profile, format, dev_gui } => {
            build(profile, format, dev_gui)?;
        }
        Commands::Install { skip_build, profile, format, dev_gui } => {
            if !skip_build {
                build(profile, format, dev_gui)?;
            }
            install(format)?;
        }
        Commands::GenTypes => {
            gen_types()?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// High-level: build + bundle for the current platform
// ---------------------------------------------------------------------------

fn build(profile: Profile, format: Format, dev_gui: bool) -> anyhow::Result<()> {
    let host = HostOs::detect()?;
    let profile_str = profile.as_str();
    let project_root = project_root()?;
    let info = BundleInfo::load(&project_root)?;

    gen_types()?;
    build_frontend()?;
    build_plugin(profile_str, dev_gui)?;

    let dylib_src = project_root
        .join("target")
        .join(profile_str)
        .join(host.dylib_filename());
    if !dylib_src.exists() {
        return Err(anyhow::anyhow!(
            "Built dylib not found at {:?} — cargo build may have failed silently",
            dylib_src
        ));
    }

    let bundle_dir = project_root.join("target").join("bundle");
    fs::create_dir_all(&bundle_dir)?;

    if format.wants_clap() {
        create_clap_bundle(host, &info, &dylib_src, &bundle_dir)?;
    }
    if format.wants_vst3() {
        create_vst3_bundle(host, &info, &dylib_src, &bundle_dir)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Install — copies the freshly-built bundles into the user's plugin dirs
// ---------------------------------------------------------------------------

fn install(format: Format) -> anyhow::Result<()> {
    let host = HostOs::detect()?;
    let project_root = project_root()?;
    let info = BundleInfo::load(&project_root)?;
    let bundle_dir = project_root.join("target").join("bundle");
    let (clap_user_dir, vst3_user_dir) = host.user_plugin_dirs()?;

    // Known historical bundle names that may still be lingering from older
    // builds (nih-plug era used lowercase). Sweep them alongside the current
    // name so DAWs don't end up scanning two copies.
    const LEGACY_CLAP_NAMES: &[&str] = &["simply_droplets.clap"];
    const LEGACY_VST3_NAMES: &[&str] = &["simply_droplets.vst3"];

    if format.wants_clap() {
        let name = info.clap_bundle_name();
        let src = bundle_dir.join(&name);
        install_artifact(&src, &clap_user_dir, &name, LEGACY_CLAP_NAMES, "CLAP")?;
    }
    if format.wants_vst3() {
        let name = info.vst3_bundle_name();
        let src = bundle_dir.join(&name);
        install_artifact(&src, &vst3_user_dir, &name, LEGACY_VST3_NAMES, "VST3")?;
    }
    println!();
    println!("✨ Installation complete. Restart your DAW and rescan plugins.");
    Ok(())
}

/// Copy `src` (file or directory) into `dest_dir` as `bundle_name`, atomically
/// replacing any existing entry. Also removes any legacy bundle names so DAWs
/// don't scan both old and new copies.
///
/// Install order is staging-then-swap so a failed copy never leaves the user
/// without a working plugin: the new bundle is copied into a sibling `.new`
/// path first, and only once that succeeds do we remove the old install
/// (plus any legacy-name installs) and rename the staged copy into place.
fn install_artifact(
    src: &Path,
    dest_dir: &Path,
    bundle_name: &str,
    legacy_names: &[&str],
    kind: &str,
) -> anyhow::Result<()> {
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "{} bundle not found at {:?}. Run `cargo xtask build` first.",
            kind,
            src
        ));
    }
    fs::create_dir_all(dest_dir)?;

    let dest = dest_dir.join(bundle_name);
    let staged = dest_dir.join(format!("{}.new", bundle_name));

    // Clean any stale staging artifact from a previous failed run.
    remove_any(&staged)?;

    // Copy first — if this fails, the old install is still intact.
    if let Err(e) = copy_any(src, &staged) {
        let _ = remove_any(&staged);
        return Err(e);
    }

    // Copy succeeded. Now it's safe to remove the old install + legacy names
    // and swap the staged copy into place.
    for name in std::iter::once(bundle_name).chain(legacy_names.iter().copied()) {
        let stale = dest_dir.join(name);
        if stale.exists() {
            println!("🗑️  Removing existing {} → {}", kind, stale.display());
            remove_any(&stale)?;
        }
    }
    fs::rename(&staged, &dest)?;
    println!("✅ Installed {} → {}", kind, dest.display());
    Ok(())
}

fn remove_any(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn copy_any(src: &Path, dest: &Path) -> anyhow::Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dest)
    } else {
        fs::copy(src, dest)?;
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared build steps
// ---------------------------------------------------------------------------

fn project_root() -> anyhow::Result<PathBuf> {
    let current_dir = env::current_dir()?;
    Ok(if current_dir.file_name().and_then(|n| n.to_str()) == Some("xtask") {
        current_dir.parent().unwrap().to_path_buf()
    } else {
        current_dir
    })
}

fn gen_types() -> anyhow::Result<()> {
    println!("Generating TypeScript types from Rust...");
    let project_root = project_root()?;
    let types_dir = project_root.join("frontend/src/types");
    fs::create_dir_all(&types_dir)?;

    let status = Command::new("cargo")
        .arg("test")
        .arg("export_bindings")
        .env("TS_RS_EXPORT_DIR", "frontend/src/types")
        .current_dir(&project_root)
        .status()?;

    if !status.success() {
        return Err(anyhow::anyhow!("Failed to generate TypeScript types"));
    }

    println!("TypeScript types generated at: {}", types_dir.display());
    Ok(())
}

fn build_frontend() -> anyhow::Result<()> {
    let project_root = project_root()?;
    let frontend_dir = project_root.join("frontend");

    if !frontend_dir.exists() {
        println!("Frontend directory not found, skipping frontend build");
        return Ok(());
    }

    println!("Building React frontend...");

    let node_modules = frontend_dir.join("node_modules");
    if !node_modules.exists() {
        println!("Installing frontend dependencies...");
        let npm_install = Command::new(npm_cmd())
            .arg("install")
            .current_dir(&frontend_dir)
            .status()?;

        if !npm_install.success() {
            return Err(anyhow::anyhow!("Failed to install frontend dependencies"));
        }
    }

    println!("Building React bundle with webpack...");
    let npm_build = Command::new(npm_cmd())
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

/// npm ships as `npm.cmd` on Windows; `npm` on Unix.
fn npm_cmd() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn build_plugin(profile: &str, dev_gui: bool) -> anyhow::Result<()> {
    println!("Building plugin...");
    let project_root = project_root()?;

    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(&project_root);

    if profile == "release" {
        cmd.arg("--release");
    }

    if dev_gui {
        cmd.arg("--features").arg("dev-gui");
    }

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

// ---------------------------------------------------------------------------
// Bundle creation — platform-specific layout
// ---------------------------------------------------------------------------

/// CLAP bundle layout:
///   macOS:   Simply Droplets.clap/Contents/MacOS/Simply Droplets
///   Linux:   Simply Droplets.clap              (flat .so renamed)
///   Windows: Simply Droplets.clap              (flat .dll renamed)
fn create_clap_bundle(
    host: HostOs,
    info: &BundleInfo,
    dylib_src: &Path,
    bundle_dir: &Path,
) -> anyhow::Result<()> {
    let bundle_path = bundle_dir.join(info.clap_bundle_name());
    remove_any(&bundle_path)?;

    match host {
        HostOs::Macos => {
            write_macos_bundle(&bundle_path, dylib_src, info, &info.clap_bundle_id)?;
        }
        HostOs::Linux | HostOs::Windows => {
            fs::copy(dylib_src, &bundle_path)?;
        }
    }
    println!("Created CLAP bundle at: {}", bundle_path.display());
    Ok(())
}

/// VST3 bundle layout (VST3 spec requires a bundle directory on all platforms):
///   macOS:   Simply Droplets.vst3/Contents/MacOS/Simply Droplets
///   Linux:   Simply Droplets.vst3/Contents/<arch>-linux/Simply Droplets.so
///   Windows: Simply Droplets.vst3/Contents/<arch>-win/Simply Droplets.vst3
fn create_vst3_bundle(
    host: HostOs,
    info: &BundleInfo,
    dylib_src: &Path,
    bundle_dir: &Path,
) -> anyhow::Result<()> {
    let bundle_path = bundle_dir.join(info.vst3_bundle_name());
    remove_any(&bundle_path)?;

    match host {
        HostOs::Macos => {
            write_macos_bundle(&bundle_path, dylib_src, info, &info.vst3_bundle_id)?;
        }
        HostOs::Linux => {
            let arch_dir = bundle_path.join("Contents").join(host.vst3_arch_subdir());
            fs::create_dir_all(&arch_dir)?;
            // Per VST3 spec, the inner .so mirrors the bundle name (with .so)
            fs::copy(dylib_src, arch_dir.join(format!("{}.so", info.display_name)))?;
        }
        HostOs::Windows => {
            let arch_dir = bundle_path.join("Contents").join(host.vst3_arch_subdir());
            fs::create_dir_all(&arch_dir)?;
            // Per VST3 spec, the inner module has a .vst3 extension (a renamed DLL)
            fs::copy(dylib_src, arch_dir.join(format!("{}.vst3", info.display_name)))?;
        }
    }
    println!("Created VST3 bundle at: {}", bundle_path.display());
    Ok(())
}

/// macOS bundle builder — shared by CLAP + VST3. Writes `Contents/MacOS/<exe>`,
/// an Info.plist, a PkgInfo stub, fixes dylib install-names for a self-contained
/// bundle, and applies an ad-hoc codesign so DAWs will load it.
fn write_macos_bundle(
    bundle_path: &Path,
    dylib_src: &Path,
    info: &BundleInfo,
    bundle_id: &str,
) -> anyhow::Result<()> {
    let contents = bundle_path.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)?;

    let exe = macos.join(&info.display_name);
    fs::copy(dylib_src, &exe)?;
    fix_dylib_paths(&exe, dylib_src)?;

    fs::write(contents.join("Info.plist"), info_plist_xml(info, bundle_id))?;
    fs::write(contents.join("PkgInfo"), "BNDL????")?;

    // Ad-hoc codesign so the bundle loads on modern macOS.
    match Command::new("codesign")
        .args(["-s", "-", bundle_path.to_str().unwrap()])
        .status()
    {
        Ok(s) if !s.success() => println!("Warning: codesign returned non-zero"),
        Err(_) => println!("Warning: codesign not available, bundle not signed"),
        _ => {}
    }
    Ok(())
}

fn info_plist_xml(info: &BundleInfo, bundle_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>{name}</string>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
</dict>
</plist>"#,
        name = info.display_name,
        bundle_id = bundle_id,
        version = info.version,
    )
}

fn fix_dylib_paths(dylib_path: &Path, original_dylib: &Path) -> anyhow::Result<()> {
    let new_id = format!(
        "@loader_path/{}",
        dylib_path.file_name().unwrap().to_str().unwrap()
    );

    let _ = Command::new("install_name_tool")
        .args(["-id", &new_id, dylib_path.to_str().unwrap()])
        .status();

    let _ = Command::new("install_name_tool")
        .args([
            "-change",
            original_dylib.to_str().unwrap(),
            &new_id,
            dylib_path.to_str().unwrap(),
        ])
        .status();

    Ok(())
}
