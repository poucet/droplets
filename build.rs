//! Build script — exposes bundle metadata from Cargo.toml as compile-time
//! env vars so `lib.rs` can build its `PluginDescriptor` without hardcoding
//! strings. Keeps one source of truth: `[package.metadata.bundle]` is already
//! parsed by xtask for bundle naming, and by emitting the same values here
//! the plugin binary and the bundle folder end up carrying identical
//! identifiers.
//!
//! The parse is a small hand-rolled scanner rather than a `toml` dep so this
//! doesn't drag a full toml parser into the build graph.

use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set by cargo");
    let cargo_toml_path = PathBuf::from(&manifest_dir).join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", cargo_toml_path.display());

    let text = fs::read_to_string(&cargo_toml_path)
        .expect("failed to read Cargo.toml");

    let section = extract_section(&text, "package.metadata.bundle")
        .expect("Cargo.toml missing [package.metadata.bundle] section");

    // Required: display_name, vendor, clap_bundle_id. Missing values cause a
    // compile error rather than silently falling back — we want this table
    // to be the canonical source.
    for key in ["display_name", "vendor", "clap_bundle_id"] {
        let value = extract_string(&section, key)
            .unwrap_or_else(|| panic!("Cargo.toml [package.metadata.bundle] missing '{}'", key));
        let env_var = format!("DROPLETS_{}", key.to_uppercase());
        println!("cargo:rustc-env={}={}", env_var, value);
    }
}

/// Return the body of a `[section.name]` table (from the header line up to
/// the next `[…]` header or EOF). The match is exact — nested tables like
/// `[package.metadata.bundle.macos]` would be sub-sections beyond the end.
fn extract_section<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let header = format!("[{}]", name);
    let start = text.find(&header)? + header.len();
    let rest = &text[start..];
    // Find the next top-level `[` on its own line.
    let mut end = rest.len();
    for (i, line) in rest.match_indices('\n') {
        let after_newline = &rest[i + line.len()..];
        if after_newline.trim_start().starts_with('[') {
            end = i;
            break;
        }
    }
    Some(&rest[..end])
}

/// Pull a `key = "value"` string out of a table body. Ignores leading
/// whitespace and comments, and handles the common case of a quoted string
/// on one line. Doesn't cover escape sequences beyond the obvious (`\"`),
/// because none of our metadata values need them.
fn extract_string(section: &str, key: &str) -> Option<String> {
    for raw_line in section.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, rest)) = line.split_once('=') else { continue };
        if k.trim() != key {
            continue;
        }
        let rest = rest.trim();
        // Strip trailing inline comment, if any.
        let rest = match rest.find('#') {
            Some(hash) if !inside_quoted_string(rest, hash) => rest[..hash].trim(),
            _ => rest,
        };
        let rest = rest.strip_prefix('"')?;
        let rest = rest.strip_suffix('"')?;
        return Some(rest.replace("\\\"", "\""));
    }
    None
}

/// `#` inside a quoted string shouldn't be treated as a comment delimiter.
/// Good enough heuristic: count unescaped quotes before the `#`.
fn inside_quoted_string(s: &str, idx: usize) -> bool {
    let before = &s[..idx];
    let quotes = before.matches('"').count() - before.matches("\\\"").count();
    quotes % 2 == 1
}
