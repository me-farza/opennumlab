use eframe::egui::{text::LayoutJob, Color32, FontId, TextFormat};

const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "classdef",
    "continue",
    "else",
    "elseif",
    "end",
    "for",
    "function",
    "global",
    "if",
    "otherwise",
    "parfor",
    "persistent",
    "return",
    "switch",
    "try",
    "while",
    "properties",
    "methods",
    "events",
    "enumeration",
    "endfor",
    "endif",
];

pub fn highlight(text: &str, font_size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let ordinary = Color32::from_rgb(40, 48, 61);
    let mut cursor = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    while cursor < chars.len() {
        let start = cursor;
        let color = match chars[cursor].1 {
            '%' => {
                while cursor < chars.len() && chars[cursor].1 != '\n' {
                    cursor += 1;
                }
                Color32::from_rgb(49, 123, 69)
            }
            '"' => {
                cursor += 1;
                while cursor < chars.len() {
                    if chars[cursor].1 == '"' {
                        cursor += 1;
                        if cursor < chars.len() && chars[cursor].1 == '"' {
                            cursor += 1;
                        } else {
                            break;
                        }
                    } else {
                        cursor += 1;
                    }
                }
                Color32::from_rgb(151, 60, 136)
            }
            '\'' if start == 0
                || matches!(chars[start - 1].1, '=' | '(' | '[' | ',' | ';' | '\n' | ' ') =>
            {
                cursor += 1;
                while cursor < chars.len() {
                    if chars[cursor].1 == '\'' {
                        cursor += 1;
                        if cursor < chars.len() && chars[cursor].1 == '\'' {
                            cursor += 1;
                        } else {
                            break;
                        }
                    } else {
                        cursor += 1;
                    }
                }
                Color32::from_rgb(151, 60, 136)
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                cursor += 1;
                while cursor < chars.len()
                    && (chars[cursor].1.is_alphanumeric() || chars[cursor].1 == '_')
                {
                    cursor += 1;
                }
                let end = chars.get(cursor).map(|c| c.0).unwrap_or(text.len());
                if KEYWORDS.contains(&&text[chars[start].0..end]) {
                    Color32::from_rgb(37, 89, 171)
                } else {
                    ordinary
                }
            }
            c if c.is_ascii_digit() => {
                cursor += 1;
                while cursor < chars.len() && matches!(chars[cursor].1, '0'..='9' | '.' | 'e' | 'E')
                {
                    cursor += 1;
                }
                Color32::from_rgb(177, 88, 29)
            }
            _ => {
                cursor += 1;
                ordinary
            }
        };
        let begin = chars[start].0;
        let end = chars.get(cursor).map(|c| c.0).unwrap_or(text.len());
        job.append(
            &text[begin..end],
            0.0,
            TextFormat {
                font_id: FontId::monospace(font_size),
                color,
                ..Default::default()
            },
        );
    }
    job
}

/// A section starts at the last %% header before the cursor and ends at the next.
pub fn section_at(text: &str, cursor_character: usize) -> &str {
    let cursor_byte = text
        .char_indices()
        .nth(cursor_character)
        .map(|c| c.0)
        .unwrap_or(text.len());
    let mut boundaries = vec![0];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("%%") && offset > 0 {
            boundaries.push(offset);
        }
        offset += line.len();
    }
    boundaries.push(text.len());
    let start = boundaries[..boundaries.len() - 1]
        .iter()
        .rev()
        .find(|&&offset| offset <= cursor_byte)
        .copied()
        .unwrap_or(0);
    let end = boundaries
        .iter()
        .find(|&&offset| offset > cursor_byte)
        .copied()
        .unwrap_or(text.len());
    &text[start..end]
}

pub fn quote_matlab(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_current_section_with_unicode_cursor() {
        let code = "%% first\nα = 1;\n%% second\nb = 2;\n%% third\nc = 3;";
        let cursor = code[..code.find("b =").unwrap()].chars().count();
        assert_eq!(section_at(code, cursor), "%% second\nb = 2;\n");
        assert_eq!(section_at(code, 2), "%% first\nα = 1;\n");
        assert_eq!(section_at(code, code.chars().count()), "%% third\nc = 3;");
    }

    #[test]
    fn quotes_paths_without_executing_embedded_commands() {
        assert_eq!(quote_matlab("/tmp/a'b.mat"), "'/tmp/a''b.mat'");
    }
}
