/// Formats a version string including the version (`HOF_VERSION`), git hash, and build date.
///
/// Version, git hash and build date are expected to be embedded by the binary's `build.rs`
/// via `cargo:rustc-env`.
pub fn format_version(version: &str, git_hash: &str, build_date: &str) -> String {
    format!("v{version} ({git_hash}, {build_date})")
}
