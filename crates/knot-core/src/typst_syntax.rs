//! Emit Typst syntax for text produced at runtime (code, outputs, messages).
//!
//! Runtime text must never be interpreted as Typst markup: an R warning
//! mentioning `df$x` would otherwise enter math mode, and code containing a
//! triple backtick would close its raw block early.

use crate::path_utils::escape_typst_string;

/// A Typst string literal: `"…"` with escapes.
pub(crate) fn string_literal(value: &str) -> String {
    format!("\"{}\"", escape_typst_string(value))
}

/// A content block displaying `value` literally: `[#"…"]`.
pub(crate) fn text_content(value: &str) -> String {
    format!("[#{}]", string_literal(value))
}

/// A raw block whose fence is longer than any backtick run in `content`.
///
/// The closing fence goes on its own line; Typst drops that final line break.
pub(crate) fn raw_block(lang: &str, content: &str) -> String {
    let fence = "`".repeat(longest_backtick_run(content).max(2) + 1);
    format!("{fence}{lang}\n{content}\n{fence}")
}

/// Inline raw text: `` `…` `` when possible, `#raw("…");` otherwise.
pub(crate) fn inline_raw(value: &str) -> String {
    if value.contains('`') {
        format!("#raw({});", string_literal(value))
    } else {
        format!("`{value}`")
    }
}

/// Inline text inserted in markup (or math): verbatim when it contains no
/// markup syntax, otherwise as a string expression. The trailing `;` ends the
/// expression, so following text such as `.x` or `(1)` is not parsed as a
/// field access or a call.
///
/// `-` stays verbatim on purpose: markup renders `-1.5` with a true minus
/// sign. `~` is escaped because markup turns it into a non-breaking space
/// (an R formula `y ~ x` would lose its tilde).
pub(crate) fn inline_text(value: &str) -> String {
    const MARKUP: &[char] = &['\\', '#', '$', '*', '_', '`', '<', '>', '@', '[', ']', '~'];
    let comment = value.contains("//") || value.contains("/*");
    if value.contains(MARKUP) || comment || value.contains('\n') {
        format!("#{};", string_literal(value))
    } else {
        value.to_string()
    }
}

fn longest_backtick_run(content: &str) -> usize {
    content.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_block_fence_outgrows_content() {
        assert_eq!(raw_block("r", "x <- 1"), "```r\nx <- 1\n```");
        assert_eq!(
            raw_block("python", "s = \"```\""),
            "````python\ns = \"```\"\n````"
        );
        assert_eq!(raw_block("", "`````"), "``````\n`````\n``````");
    }

    #[test]
    fn text_content_escapes_string_syntax() {
        assert_eq!(text_content("df$x"), "[#\"df$x\"]");
        assert_eq!(text_content("a \"b\"\\"), "[#\"a \\\"b\\\"\\\\\"]");
    }

    #[test]
    fn inline_values_are_verbatim_only_when_safe() {
        assert_eq!(inline_text("150"), "150");
        assert_eq!(inline_text("Alice"), "Alice");
        assert_eq!(inline_text("x_1"), "#\"x_1\";");
        assert_eq!(inline_text("-1.5"), "-1.5");
        assert_eq!(inline_text("y ~ x"), "#\"y ~ x\";");
        assert_eq!(inline_text("https://x"), "#\"https://x\";");
        assert_eq!(inline_raw("[1] 1 2"), "`[1] 1 2`");
        assert_eq!(inline_raw("a`b"), "#raw(\"a`b\");");
    }
}
