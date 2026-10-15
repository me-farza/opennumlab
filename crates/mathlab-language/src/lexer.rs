// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Result, error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Word,
    Symbol,
    Literal,
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
    pub line: usize,
}

#[derive(Debug)]
pub(crate) struct Statement {
    pub tokens: Vec<Token>,
}

impl Statement {
    pub fn first_word<'a>(&self, source: &'a str) -> Option<&'a str> {
        let token = self.tokens.first()?;
        (token.kind == TokenKind::Word).then(|| &source[token.start..token.end])
    }
}

/// Statement boundaries ignore strings, comments, continuations, and bracketed
/// expressions. Only the first identifier of each statement can be a block word.
pub(crate) fn statements(source: &str) -> Result<Vec<Statement>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut tokens = Vec::new();
    let mut delimiters = Vec::new();
    let mut index = usize::from(bytes.starts_with(&[0xef, 0xbb, 0xbf])) * 3;
    let mut line = 1;
    let mut line_start = index;
    let mut expression_end = false;
    let mut whitespace = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\n' {
            if delimiters.is_empty() {
                finish(&mut result, &mut tokens);
                expression_end = false;
            }
            line += 1;
            index += 1;
            line_start = index;
            whitespace = true;
            continue;
        }
        if byte.is_ascii_whitespace() {
            index += 1;
            whitespace = true;
            continue;
        }
        if byte == b'!' && tokens.is_empty() && delimiters.is_empty() {
            // MATLAB shell escapes consume the whole physical line, including
            // quoted semicolons and words that resemble language block syntax.
            let start = index;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Literal,
                start,
                end: index,
                line,
            });
            continue;
        }
        if matches!(byte, b'%' | b'#') {
            if bytes.get(index + 1) == Some(&b'{')
                && line_is_whitespace(&bytes[line_start..index])
                && marker_ends_line(bytes, index + 2)
            {
                let marker = byte;
                let opening_line = line;
                index += 2;
                let mut depth = 1;
                while index < bytes.len() {
                    if bytes[index] == b'\n' {
                        line += 1;
                        index += 1;
                        line_start = index;
                        continue;
                    }
                    if bytes[index] == marker
                        && line_is_whitespace(&bytes[line_start..index])
                        && marker_ends_line(bytes, index + 2)
                    {
                        match bytes.get(index + 1) {
                            Some(b'{') => {
                                depth += 1;
                                index += 2;
                                continue;
                            }
                            Some(b'}') => {
                                depth -= 1;
                                index += 2;
                                if depth == 0 {
                                    break;
                                }
                                continue;
                            }
                            _ => {}
                        }
                    }
                    index += 1;
                }
                if depth != 0 {
                    return Err(error(opening_line, "Unterminated block comment"));
                }
                whitespace = true;
            } else {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            continue;
        }
        if bytes[index..].starts_with(b"...") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            if index < bytes.len() {
                line += 1;
                index += 1;
                line_start = index;
            }
            whitespace = true;
            continue;
        }
        let start = index;
        let token_line = line;
        let matrix_string = whitespace && matches!(delimiters.last(), Some(b'[' | b'{'));
        let command_string = whitespace
            && delimiters.is_empty()
            && tokens
                .first()
                .is_some_and(|token: &Token| token.kind == TokenKind::Word)
            && tokens.iter().all(|token| token.kind != TokenKind::Symbol);
        if byte == b'"' || (byte == b'\'' && (!expression_end || matrix_string || command_string)) {
            let quote = byte;
            index += 1;
            let mut closed = false;
            while index < bytes.len() {
                if bytes[index] == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        index += 2;
                        continue;
                    }
                    index += 1;
                    closed = true;
                    break;
                }
                if bytes[index] == b'\n' {
                    return Err(error(token_line, "Unterminated quoted literal"));
                }
                index += 1;
            }
            if !closed {
                return Err(error(token_line, "Unterminated quoted literal"));
            }
            tokens.push(Token {
                kind: TokenKind::Literal,
                start,
                end: index,
                line: token_line,
            });
            expression_end = true;
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Word,
                start,
                end: index,
                line: token_line,
            });
            // A file whose first token is function/classdef belongs to Octave's
            // native parser. Do not reinterpret dialect-specific syntax inside.
            if result.is_empty()
                && tokens.len() == 1
                && matches!(&source[start..index], "function" | "classdef")
            {
                finish(&mut result, &mut tokens);
                return Ok(result);
            }
            expression_end = true;
        } else if byte.is_ascii_digit() {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_digit()
                    || (bytes[index] == b'.'
                        && !bytes[index..].starts_with(b"...")
                        && !bytes[index..].starts_with(b".'")))
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Literal,
                start,
                end: index,
                line: token_line,
            });
            expression_end = true;
        } else {
            index += 1;
            match byte {
                b'(' | b'[' | b'{' => {
                    delimiters.push(byte);
                    expression_end = false;
                }
                b')' | b']' | b'}' => {
                    let expected = match byte {
                        b')' => b'(',
                        b']' => b'[',
                        _ => b'{',
                    };
                    if delimiters.pop() != Some(expected) {
                        return Err(error(token_line, "Unmatched expression delimiter"));
                    }
                    expression_end = true;
                }
                b';' | b',' if delimiters.is_empty() => {
                    finish(&mut result, &mut tokens);
                    expression_end = false;
                    whitespace = false;
                    continue;
                }
                b'\'' => expression_end = true,
                // Nonconjugate transpose `.'` is a single postfix operator.
                b'.' if bytes.get(index) == Some(&b'\'') && expression_end => {
                    index += 1;
                    expression_end = true;
                }
                _ => expression_end = false,
            }
            // Symbols may include a UTF-8 character; keep token spans valid.
            while index < bytes.len() && !source.is_char_boundary(index) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Symbol,
                start,
                end: index,
                line: token_line,
            });
        }
        whitespace = false;
    }
    if !delimiters.is_empty() {
        return Err(error(line, "Unclosed expression delimiter"));
    }
    finish(&mut result, &mut tokens);
    Ok(result)
}

fn line_is_whitespace(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_whitespace)
}

fn marker_ends_line(bytes: &[u8], start: usize) -> bool {
    let end = bytes[start.min(bytes.len())..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset);
    line_is_whitespace(&bytes[start.min(bytes.len())..end])
}

fn finish(result: &mut Vec<Statement>, tokens: &mut Vec<Token>) {
    if !tokens.is_empty() {
        result.push(Statement {
            tokens: std::mem::take(tokens),
        });
    }
}
