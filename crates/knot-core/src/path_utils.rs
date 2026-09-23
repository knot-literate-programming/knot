//! Path utilities for code generation
//!
//! Native paths remain `Path`/`PathBuf` until a code-generation boundary.
//! R/Python receive native paths; published Typst paths use `/` separators.

use std::path::Path;

/// Escape a path for safe use in code strings
///
/// Converts Windows backslashes to double-backslashes to prevent
/// escape sequence issues when embedding paths in R/Python code.
/// It also escapes single and double quotes to prevent code injection.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use knot_core::path_utils::escape_path_for_code;
///
/// let path = Path::new(r"C:\Users\data.csv");
/// let escaped = escape_path_for_code(path);
/// assert_eq!(escaped, r"C:\\Users\\data.csv");
/// ```
pub fn escape_path_for_code(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_escape_path_windows() {
        let path = Path::new(r"C:\Users\file.txt");
        assert_eq!(escape_path_for_code(path), r"C:\\Users\\file.txt");
    }

    #[test]
    fn test_escape_path_unix() {
        let path = Path::new("/home/user/file.txt");
        assert_eq!(escape_path_for_code(path), "/home/user/file.txt");
    }

    #[test]
    fn test_escape_path_with_single_quotes() {
        let path = Path::new(r"C:\User's\data.csv");
        assert_eq!(escape_path_for_code(path), r"C:\\User\'s\\data.csv");
    }

    #[test]
    fn test_escape_path_with_double_quotes() {
        let path = Path::new(r#"C:\Users\"special"\file.txt"#);
        assert_eq!(
            escape_path_for_code(path),
            r#"C:\\Users\\\"special\"\\file.txt"#
        );
    }
}

/// Escape a native path in an intermediate Typst string, before artifact staging.
/// This preserves its spelling so publication can find the actual filesystem path.
pub(crate) fn escape_typst_path(path: &Path) -> String {
    escape_typst_string(&path.to_string_lossy())
}

/// Escape string contents without interpreting them as a filesystem path.
pub(crate) fn escape_typst_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Encode a project-relative path for Typst's platform-independent filesystem.
/// `Path::components` recognizes native separators; only those separators change.
/// Absolute cache paths must first be staged beneath the document root.
pub(crate) fn published_typst_path(path: &Path) -> anyhow::Result<String> {
    use std::path::Component;
    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(name) => name
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Non-UTF-8 Typst path")),
            _ => anyhow::bail!("Expected a relative artifact path: {}", path.display()),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    if components.is_empty() {
        anyhow::bail!("Empty Typst artifact path");
    }
    Ok(escape_typst_string(&components.join("/")))
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn native_components_become_portable_typst_separators() {
        let path = Path::new("_knot_files")
            .join("hash")
            .join("été \"data\".json");
        assert_eq!(
            published_typst_path(&path).unwrap(),
            "_knot_files/hash/été \\\"data\\\".json"
        );
    }

    #[test]
    fn published_paths_must_stay_relative() {
        assert!(published_typst_path(Path::new("../data.json")).is_err());
        assert!(published_typst_path(Path::new("")).is_err());
        assert!(published_typst_path(&std::env::current_dir().unwrap()).is_err());
    }

    #[test]
    fn code_strings_escape_quotes_and_control_characters() {
        assert_eq!(
            escape_path_for_code(Path::new("a'b\"c\nd\r\t")),
            "a\\'b\\\"c\\nd\\r\\t"
        );
    }

    #[cfg(unix)]
    #[test]
    fn literal_unix_backslashes_are_not_mistaken_for_separators() {
        let path = Path::new("_knot_files").join(r"a\b.json");
        assert_eq!(
            published_typst_path(&path).unwrap(),
            r"_knot_files/a\\b.json"
        );
    }

    #[cfg(windows)]
    #[test]
    fn extended_windows_paths_are_preserved_for_interpreters_not_published() {
        let path = Path::new(r"\\?\C:\data\résultats.json");
        assert_eq!(
            escape_path_for_code(path),
            r"\\\\?\\C:\\data\\résultats.json"
        );
        assert!(published_typst_path(path).is_err());
        let unc = Path::new(r"\\?\UNC\server\share\data.json");
        assert!(published_typst_path(unc).is_err());
        assert_eq!(
            escape_path_for_code(unc),
            r"\\\\?\\UNC\\server\\share\\data.json"
        );
    }
}
