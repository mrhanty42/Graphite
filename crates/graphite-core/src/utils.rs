use std::path::Path;

/// Check if a file has the correct dynamic library extension for the current platform.
pub fn has_dynlib_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(std::env::consts::DLL_EXTENSION))
        .unwrap_or(false)
}
