// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Name,
    Number,
    Newline,
    Comma,
    Semi,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Plus,
    Minus,
    Not,
    Star,
    Slash,
    Backslash,
    DotStar,
    DotSlash,
    DotBackslash,
    Power,
    DotPower,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    And,
    Or,
    AndAnd,
    OrOr,
    Assign,
    Colon,
    Quote,
    DotQuote,
    Eof,
    Dot,
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Token {
    pub kind: Kind,
    pub span: ProgramSpanV1,
    pub space: bool,
}
pub(super) fn lex(s: &str, b: &mut Budget<'_>) -> ParseResult<Vec<Token>> {
    lex_mode(s, b, false)
}
pub(super) fn lex_class(s: &str, b: &mut Budget<'_>) -> ParseResult<Vec<Token>> {
    lex_mode(s, b, true)
}
fn lex_mode(s: &str, b: &mut Budget<'_>, members: bool) -> ParseResult<Vec<Token>> {
    let origin = ProgramSpanV1 {
        start: 0,
        end: 0,
        start_line: 1,
        end_line: 1,
    };
    let mut out = Vec::new();
    b.reserve(&mut out, s.len() + 1, origin)?;
    let bytes = s.as_bytes();
    let mut i = if s.starts_with('\u{feff}') { 3 } else { 0 };
    let mut line = 1;
    let mut space = false;
    let mut comment = 0usize;
    let mut line_start = true;
    while i < bytes.len() {
        let start = i;
        let sl = line;
        let span = ProgramSpanV1 {
            start,
            end: start + 1,
            start_line: line,
            end_line: line,
        };
        b.check(span)?;
        let c = bytes[i];
        if c == b' ' || c == b'\t' {
            i += 1;
            space = true;
            continue;
        }
        if c == b'\r' || c == b'\n' {
            i += 1;
            if c == b'\r' && bytes.get(i) == Some(&b'\n') {
                i += 1
            }
            line += 1;
            line_start = true;
            if comment == 0 {
                out.push(Token {
                    kind: Kind::Newline,
                    span: ProgramSpanV1 {
                        start,
                        end: i,
                        start_line: sl,
                        end_line: line,
                    },
                    space,
                });
            }
            space = false;
            continue;
        }
        if line_start && c == b'%' && matches!(bytes.get(i + 1), Some(b'{') | Some(b'}')) {
            let opening = bytes[i + 1] == b'{';
            let mut end = i + 2;
            while matches!(bytes.get(end), Some(b' ') | Some(b'\t')) {
                b.check(span)?;
                end += 1
            }
            if end == bytes.len() || matches!(bytes[end], b'\r' | b'\n') {
                if opening {
                    comment = b.count(comment, MAX_PROGRAM_NESTING_V1, span)?;
                } else if comment > 0 {
                    comment -= 1;
                } else {
                    return Err(b.error(span, ProgramParseErrorCodeV1::Syntax));
                }
                i = end;
                continue;
            }
        }
        if comment > 0 || c == b'%' {
            while i < bytes.len() && !matches!(bytes[i], b'\r' | b'\n') {
                b.check(span)?;
                i += 1
            }
            continue;
        }
        line_start = false;
        if bytes[i..].starts_with(b"...") {
            i += 3;
            while i < bytes.len() && !matches!(bytes[i], b'\r' | b'\n') {
                b.check(span)?;
                i += 1
            }
            if i < bytes.len() {
                let cr = bytes[i] == b'\r';
                i += 1;
                if cr && bytes.get(i) == Some(&b'\n') {
                    i += 1
                }
                line += 1;
                line_start = true;
            }
            space = true;
            continue;
        }
        let kind;
        if c.is_ascii_alphabetic() || c == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                b.check(span)?;
                i += 1
            }
            kind = Kind::Name;
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                b.check(span)?;
                i += 1
            }
            if bytes.get(i) == Some(&b'.')
                && !matches!(
                    bytes.get(i + 1),
                    Some(b'*' | b'/' | b'\\' | b'^' | b'\'' | b'.')
                )
            {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    b.check(span)?;
                    i += 1
                }
            }
            if matches!(bytes.get(i), Some(b'e' | b'E')) {
                i += 1;
                if matches!(bytes.get(i), Some(b'+' | b'-')) {
                    i += 1
                }
                let digits = i;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    b.check(span)?;
                    i += 1
                }
                if digits == i {
                    return Err(b.error(span, ProgramParseErrorCodeV1::Syntax));
                }
            }
            kind = Kind::Number;
        } else {
            let two = bytes.get(i..i + 2);
            let matched = match two {
                Some(b".*") => Some(Kind::DotStar),
                Some(b"./") => Some(Kind::DotSlash),
                Some(b".\\") => Some(Kind::DotBackslash),
                Some(b".^") => Some(Kind::DotPower),
                Some(b".'") => Some(Kind::DotQuote),
                Some(b"<=") => Some(Kind::Le),
                Some(b">=") => Some(Kind::Ge),
                Some(b"==") => Some(Kind::EqEq),
                Some(b"~=") => Some(Kind::Ne),
                Some(b"&&") => Some(Kind::AndAnd),
                Some(b"||") => Some(Kind::OrOr),
                _ => None,
            };
            if let Some(k) = matched {
                kind = k;
                i += 2
            } else {
                kind = match c {
                    b'(' => Kind::LParen,
                    b')' => Kind::RParen,
                    b'[' => Kind::LBracket,
                    b']' => Kind::RBracket,
                    b',' => Kind::Comma,
                    b';' => Kind::Semi,
                    b'+' => Kind::Plus,
                    b'-' => Kind::Minus,
                    b'~' => Kind::Not,
                    b'*' => Kind::Star,
                    b'/' => Kind::Slash,
                    b'\\' => Kind::Backslash,
                    b'^' => Kind::Power,
                    b'<' => Kind::Lt,
                    b'>' => Kind::Gt,
                    b'&' => Kind::And,
                    b'|' => Kind::Or,
                    b'=' => Kind::Assign,
                    b':' => Kind::Colon,
                    b'.' if members => Kind::Dot,
                    b'\'' => Kind::Quote,
                    _ => return Err(b.error(span, ProgramParseErrorCodeV1::UnsupportedFeature)),
                };
                i += 1
            }
        }
        out.push(Token {
            kind,
            span: ProgramSpanV1 {
                start,
                end: i,
                start_line: sl,
                end_line: line,
            },
            space,
        });
        space = false;
    }
    let span = ProgramSpanV1 {
        start: i,
        end: i,
        start_line: line,
        end_line: line,
    };
    b.check(span)?;
    if comment != 0 {
        return Err(b.error(span, ProgramParseErrorCodeV1::Syntax));
    }
    out.push(Token {
        kind: Kind::Eof,
        span,
        space,
    });
    b.stats.tokens = out.len();
    Ok(out)
}
