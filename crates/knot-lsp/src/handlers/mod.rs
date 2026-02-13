use tower_lsp::lsp_types::Position;

pub mod completion;
pub mod formatting;
pub mod hover;

/// Extract the token at the given position in the text.
/// `bidirectional` determines if we look forward as well as backward.
pub fn get_token_at_pos(text: &str, pos: Position, lang: &str, bidirectional: bool) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(pos.line as usize)?;
    let col = pos.character as usize;
    
    // Convert UTF-16 character position to character index
    let chars: Vec<char> = line.chars().collect();
    let mut char_idx = 0;
    let mut utf16_count = 0;
    for (i, c) in chars.iter().enumerate() {
        if utf16_count >= col {
            char_idx = i;
            break;
        }
        utf16_count += c.len_utf16();
        if i == chars.len() - 1 {
            char_idx = chars.len();
        }
    }

    if char_idx > chars.len() {
        return None;
    }

    let mut start = char_idx;
    while start > 0 && is_id_char(chars[start - 1], lang) {
        start -= 1;
    }

    let mut end = char_idx;
    if bidirectional {
        while end < chars.len() && is_id_char(chars[end], lang) {
            end += 1;
        }
    }

    if start == end {
        None
    } else {
        Some(chars[start..end].iter().collect())
    }
}

/// Determine if a character is part of an identifier for the given language.
pub fn is_id_char(c: char, lang: &str) -> bool {
    if lang == "r" {
        c.is_alphanumeric() || c == '_' || c == '.' || c == '$' || c == ':'
    } else {
        c.is_alphanumeric() || c == '_' || c == '.'
    }
}
