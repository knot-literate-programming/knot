//! Path utilities for code generation
//!
//! Helpers for safely embedding file paths in generated R/Python code.

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
/// use knot_core::executors::path_utils::escape_path_for_code;
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
