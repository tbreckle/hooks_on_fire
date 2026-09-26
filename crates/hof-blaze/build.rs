use std::process::Command;

fn main() {
    // Git hash with dirty flag
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);

    let git_hash = if dirty {
        format!("{git_hash}-dirty")
    } else {
        git_hash
    };

    // Build date from last commit
    let build_date = Command::new("git")
        .args(["log", "-1", "--format=%cd", "--date=short"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|d| d.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=HOF_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=HOF_BUILD_DATE={build_date}");

    // CI injects the version computed by scripts/version.sh. Cargo.toml stays at 0.0.0, so
    // local builds are marked as unofficial.
    println!("cargo:rerun-if-env-changed=HOF_VERSION");
    let version = std::env::var("HOF_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    println!("cargo:rustc-env=HOF_VERSION={version}");

    // Rerun if git state changes
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}
