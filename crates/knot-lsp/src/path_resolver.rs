use std::path::{Path, PathBuf};
use anyhow::Result;

/// Resolve a binary path by checking PATH and common installation directories
pub fn resolve_binary(name: &str) -> Result<PathBuf> {
    // 1. Try to find in system PATH
    if let Ok(path) = which::which(name) {
        return Ok(path);
    }

    // 2. Try common installation locations
    let mut search_paths = vec![
        shellexpand::tilde("~/bin").to_string(),
        shellexpand::tilde("~/.cargo/bin").to_string(),
        "/usr/local/bin".to_string(),
        "/opt/homebrew/bin".to_string(),
    ];

    // Add OS-specific paths
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            let win_path = PathBuf::from(appdata).join("Programs").join(name);
            if let Some(s) = win_path.to_str() {
                search_paths.push(s.to_string());
            }
        }
    }

    for dir in search_paths {
        let path = Path::new(&dir).join(name);
        if path.exists() && path.is_file() {
            return Ok(path);
        }
        
        // Special case for tinymist and air in VS Code extensions
        if (name == "tinymist" || name == "air") && cfg!(target_os = "macos") {
            let extensions_dir = Path::new(&shellexpand::tilde("~/.vscode/extensions").to_string()).to_path_buf();
            if extensions_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(extensions_dir) {
                    let prefix = if name == "tinymist" { "myriad-dreamin.tinymist-" } else { "posit.air-" };
                    
                    for entry in entries.flatten() {
                        let dirname = entry.file_name().to_string_lossy().to_string();
                        if dirname.starts_with(prefix) {
                            let candidate = if name == "tinymist" {
                                entry.path().join("out").join("tinymist")
                            } else {
                                // Try common air subpaths
                                let p1 = entry.path().join("bin").join("air");
                                let p2 = entry.path().join("bundled").join("bin").join("air");
                                if p2.exists() { p2 } else { p1 }
                            };

                            if candidate.exists() && candidate.is_file() {
                                return Ok(candidate);
                            }
                        }
                    }
                }
            }
        }

        // On windows, also try with .exe
        if cfg!(target_os = "windows") && !name.ends_with(".exe") {
            let path_exe = Path::new(&dir).join(format!("{}.exe", name));
            if path_exe.exists() && path_exe.is_file() {
                return Ok(path_exe);
            }
        }
    }

    anyhow::bail!("Binary '{}' not found in PATH or common locations", name)
}
