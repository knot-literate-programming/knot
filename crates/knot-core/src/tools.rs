//! Shared executable discovery for interpreters, formatters and rendering tools.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Optional executable names or paths in the `[tools]` section of `knot.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsConfig {
    /// Python interpreter (defaults to `python3`).
    pub python: Option<PathBuf>,
    /// Interactive R executable (not Rscript).
    pub r: Option<PathBuf>,
    /// Typst CLI.
    pub typst: Option<PathBuf>,
    /// Tinymist language server and preview CLI.
    pub tinymist: Option<PathBuf>,
    /// Air R formatter.
    pub air: Option<PathBuf>,
    /// Ruff Python formatter.
    pub ruff: Option<PathBuf>,
}

impl ToolsConfig {
    /// Anchor relative paths to the configuration directory; bare names use PATH.
    pub(crate) fn anchor(&mut self, root: &Path) -> Result<()> {
        for (name, value) in [
            ("python", &mut self.python),
            ("r", &mut self.r),
            ("typst", &mut self.typst),
            ("tinymist", &mut self.tinymist),
            ("air", &mut self.air),
            ("ruff", &mut self.ruff),
        ] {
            if let Some(path) = value {
                anyhow::ensure!(
                    !path.as_os_str().is_empty(),
                    "tools.{name} must not be empty"
                );
                if path.is_relative() && path.components().count() > 1 {
                    *path = root.join(&*path);
                }
            }
        }
        Ok(())
    }
}

/// Resolve an executable: explicit project setting, PATH, common directories,
/// then a client-provided fallback. Explicit invalid settings never fall back.
/// Uses `which` for platform-specific executable checks (including Windows extensions).
pub fn resolve_binary(
    name: &str,
    configured: Option<&Path>,
    client: Option<&Path>,
) -> Result<PathBuf> {
    let mut directories = Vec::new();
    if let Some(home) = dirs::home_dir() {
        directories.extend([home.join("bin"), home.join(".cargo/bin")]);
    }
    if cfg!(unix) {
        directories.extend([
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ]);
    }
    if cfg!(windows)
        && let Some(appdata) = std::env::var_os("LOCALAPPDATA")
    {
        directories.push(PathBuf::from(appdata).join("Programs").join(name));
    }
    resolve_in(
        name,
        configured,
        client,
        std::env::var_os("PATH"),
        &directories,
        &std::env::current_dir()?,
    )
}

fn resolve_in(
    name: &str,
    configured: Option<&Path>,
    client: Option<&Path>,
    paths: Option<std::ffi::OsString>,
    directories: &[PathBuf],
    cwd: &Path,
) -> Result<PathBuf> {
    if let Some(path) = configured {
        return which::which_in(path, paths.as_ref(), cwd).with_context(|| {
            format!(
                "Configured tool '{name}' is not executable or was not found: {}",
                path.display()
            )
        });
    }
    if let Ok(path) = which::which_in(name, paths.as_ref(), cwd) {
        return Ok(path);
    }
    for directory in directories {
        if let Ok(path) = which::which_in(directory.join(name), paths.as_ref(), cwd) {
            return Ok(path);
        }
    }
    if let Some(path) = client {
        return which::which_in(path, paths.as_ref(), cwd).with_context(|| {
            format!(
                "Client-provided tool '{name}' is not executable or was not found: {}",
                path.display()
            )
        });
    }
    anyhow::bail!(
        "Tool '{name}' not found in PATH or common locations; configure it in [tools] in knot.toml"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(root: &Path, directory: &str) -> PathBuf {
        let directory = root.join(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let target = directory.join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        std::fs::copy(std::env::current_exe().unwrap(), &target).unwrap();
        target
    }

    #[test]
    fn resolution_obeys_project_path_common_client_priority() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let project = executable(root, "project tools été");
        let path = executable(root, "path");
        let common = executable(root, "common");
        let client = executable(root, "client");
        let paths = std::env::join_paths([path.parent().unwrap()]).unwrap();
        let common_dirs = [common.parent().unwrap().to_path_buf()];
        assert_eq!(
            resolve_in(
                "fixture",
                Some(&project),
                Some(&client),
                Some(paths.clone()),
                &common_dirs,
                root
            )
            .unwrap(),
            project
        );
        assert_eq!(
            resolve_in(
                "fixture",
                None,
                Some(&client),
                Some(paths),
                &common_dirs,
                root
            )
            .unwrap(),
            path
        );
        assert_eq!(
            resolve_in("fixture", None, Some(&client), None, &common_dirs, root).unwrap(),
            common
        );
        assert_eq!(
            resolve_in("fixture", None, Some(&client), None, &[], root).unwrap(),
            client
        );
    }

    #[test]
    fn invalid_explicit_setting_does_not_fall_back() {
        let dir = tempfile::tempdir().unwrap();
        let client = executable(dir.path(), "client");
        let error = resolve_in(
            "fixture",
            Some(&dir.path().join("missing")),
            Some(&client),
            None,
            &[],
            dir.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Configured tool"));
    }

    #[test]
    fn bare_configured_name_uses_path() {
        let dir = tempfile::tempdir().unwrap();
        let binary = executable(dir.path(), "bin");
        let paths = std::env::join_paths([binary.parent().unwrap()]).unwrap();
        assert_eq!(
            resolve_in(
                "python3",
                Some(Path::new("fixture")),
                None,
                Some(paths),
                &[],
                dir.path()
            )
            .unwrap(),
            binary
        );
    }

    #[test]
    fn config_paths_are_relative_to_toml_not_the_source_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("chapters")).unwrap();
        std::fs::write(
            dir.path().join("knot.toml"),
            "[tools]\npython = './my env/python'\nr = 'custom-R'\ntypst = './tools/typst'\n",
        )
        .unwrap();
        let (config, _) = crate::Config::find_and_load(&dir.path().join("chapters")).unwrap();
        assert_eq!(
            config.tools.python.unwrap(),
            dir.path().join("./my env/python")
        );
        assert_eq!(config.tools.r.unwrap(), Path::new("custom-R"));
        assert_eq!(
            config.tools.typst.unwrap(),
            dir.path().join("./tools/typst")
        );
        assert!(config.tools.air.is_none());
    }

    #[test]
    fn invalid_tool_settings_are_reported_at_load_time() {
        let dir = tempfile::tempdir().unwrap();
        for settings in ["python = ''", "pyhton = 'python'"] {
            std::fs::write(
                dir.path().join("knot.toml"),
                format!("[tools]\n{settings}\n"),
            )
            .unwrap();
            assert!(crate::Config::find_and_load(dir.path()).is_err());
        }
    }
}
