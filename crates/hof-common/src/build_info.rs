/// Formats a version string including package version, git hash, and build date.
///
/// The git hash and build date are expected to be embedded by the binary's `build.rs`
/// via `cargo:rustc-env`.
pub fn format_version(pkg_version: &str, git_hash: &str, build_date: &str) -> String {
    format!("v{pkg_version} ({git_hash}, {build_date})")
}
