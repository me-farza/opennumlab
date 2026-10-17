//! Prepare MATLAB scripts containing local functions for Octave execution.
//!
//! Octave executes a script before defining functions appearing later in it.
//! This crate extracts local definitions into an isolated `private` directory,
//! retaining the script's base workspace and the original source line numbers.
//! The engine keeps materialized parents at the end of Octave's load path.
//! Generated scripts use a non-`.m` extension so they cannot be autoloaded by name.
//! Keeping all files unchanged preserves handles and local persistent state.
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod arithmetic_expression;
pub mod decimal_expression;
pub mod expression;
mod expression_core;
mod lexer;
pub mod literal_evaluation;
pub mod session_expression;

use lexer::{Statement, TokenKind, statements};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SCRIPT_ID: AtomicU64 = AtomicU64::new(0);

pub type Result<T> = std::result::Result<T, PrepareError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for PrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MATLAB script preparation at line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for PrepareError {}

/// Byte offsets and one-based line numbers in the original UTF-8 source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFunction {
    pub name: String,
    /// Leading padding preserves the original function's line and column numbers.
    pub source: String,
    pub span: SourceSpan,
}

/// The native MATLAB file form, determined by its first executable statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Script,
    Function,
    Class,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedScript {
    pub original_path: PathBuf,
    /// Definitions are replaced by spaces, preserving every newline and column.
    pub source: String,
    pub local_functions: Vec<LocalFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMapping {
    pub generated_path: PathBuf,
    pub original_path: PathBuf,
    /// Local function name, or None for the script itself. Lines need no adjustment.
    pub local_function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedScript {
    pub script_path: PathBuf,
    pub function_paths: Vec<PathBuf>,
    pub source_mapping: Vec<FileMapping>,
}

impl PreparedScript {
    /// Classify with the preparation lexer, skipping comments and a UTF-8 BOM.
    /// A script's extracted local definitions do not change its file form.
    pub fn source_kind(&self) -> Result<SourceKind> {
        Ok(
            match statements(&self.source)?
                .first()
                .and_then(|statement| statement.first_word(&self.source))
            {
                Some("function") => SourceKind::Function,
                Some("classdef") => SourceKind::Class,
                _ => SourceKind::Script,
            },
        )
    }

    pub fn is_transformed(&self) -> bool {
        !self.local_functions.is_empty()
    }

    /// Insert a same-line runtime registration statement before this token.
    /// Leading block comments and a UTF-8 BOM must keep their original positions;
    /// prepending at byte zero can turn a standalone `%{` into a line comment.
    /// Returns None for files without extracted local functions.
    pub fn scope_registration_offset(&self) -> Result<Option<usize>> {
        if !self.is_transformed() {
            return Ok(None);
        }
        Ok(statements(&self.source)?
            .first()
            .and_then(|statement| statement.tokens.first())
            .map(|token| token.start))
    }

    /// Write into an engine-owned, unique directory. Existing files are rejected.
    /// Transformed scripts get unique non-.m names to avoid Octave resolving a
    /// same-named original in the current directory while parsing private scope.
    /// The caller owns cleanup; retain all files until the session terminates.
    pub fn write_to(&self, directory: &Path) -> io::Result<MaterializedScript> {
        let filename = self.original_path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Script path has no filename")
        })?;
        fs::create_dir_all(directory)?;
        let directory = fs::canonicalize(directory)?;
        if fs::read_dir(&directory)?.next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Materialization requires an empty session directory",
            ));
        }
        let filename = if self.is_transformed() {
            let id = SCRIPT_ID.fetch_add(1, Ordering::Relaxed);
            std::ffi::OsString::from(format!(
                "__mathlab_script_{}_{id}.mlsource",
                std::process::id()
            ))
        } else {
            filename.to_owned()
        };
        let script_path = directory.join(filename);
        write_new(&script_path, &self.source)?;
        let mut source_mapping = vec![FileMapping {
            generated_path: script_path.clone(),
            original_path: self.original_path.clone(),
            local_function: None,
        }];
        let mut function_paths = Vec::new();
        if self.is_transformed() {
            let private = directory.join("private");
            fs::create_dir(&private)?;
            for function in &self.local_functions {
                let generated_path = private.join(format!("{}.m", function.name));
                write_new(&generated_path, &function.source)?;
                source_mapping.push(FileMapping {
                    generated_path: generated_path.clone(),
                    original_path: self.original_path.clone(),
                    local_function: Some(function.name.clone()),
                });
                function_paths.push(generated_path);
            }
        }
        Ok(MaterializedScript {
            script_path,
            function_paths,
            source_mapping,
        })
    }
}

fn write_new(path: &Path, source: &str) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(source.as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Function,
    If,
    For,
    While,
    Switch,
    Try,
    UnwindProtect,
    Do,
    Arguments,
}

#[derive(Debug)]
struct OpenBlock {
    kind: Block,
    line: usize,
}

/// Parse block structure without modifying identifiers or executable expressions.
/// Ordinary function/class files pass through unchanged. Script local functions
/// require explicit `end`/`endfunction`; nested functions are kept in their parent.
/// Modern scripts with executable statements between local functions are supported.
pub fn prepare_script(path: &Path, source: &str) -> Result<PreparedScript> {
    let mut prepared = PreparedScript {
        original_path: path.to_owned(),
        source: source.to_owned(),
        local_functions: Vec::new(),
    };
    let statements = statements(source)?;
    if statements.first().is_some_and(|statement| {
        matches!(statement.first_word(source), Some("function" | "classdef"))
    }) {
        return Ok(prepared);
    }
    // Avoid imposing this structural parser on ordinary Octave scripts that have
    // no local definitions; their own parser remains authoritative.
    if !statements
        .iter()
        .any(|statement| statement.first_word(source) == Some("function"))
    {
        return Ok(prepared);
    }
    let mut blocks: Vec<OpenBlock> = Vec::new();
    let mut local_start: Option<(String, usize, usize)> = None;
    let mut names = HashSet::new();
    for statement in &statements {
        let Some(keyword) = statement.first_word(source) else {
            continue;
        };
        let first = &statement.tokens[0];
        if keyword == "function" {
            if blocks.iter().any(|block| block.kind != Block::Function) {
                return Err(error(
                    first.line,
                    "Function definitions inside control blocks are unsupported",
                ));
            }
            let name = function_name(statement, source)?;
            if blocks.is_empty() {
                if !names.insert(name.clone()) {
                    return Err(error(
                        first.line,
                        format!("Duplicate local function '{name}'"),
                    ));
                }
                local_start = Some((name, first.start, first.line));
            }
            blocks.push(OpenBlock {
                kind: Block::Function,
                line: first.line,
            });
        } else if keyword == "classdef" {
            return Err(error(
                first.line,
                "Local class definitions in scripts are unsupported",
            ));
        } else if keyword == "arguments"
            && statement.tokens.get(1).is_some_and(|token| {
                token.kind != TokenKind::Symbol || &source[token.start..token.end] != "("
            })
        {
            // `arguments` is contextual: ordinary assignments such as
            // `arguments = 2` remain expressions in Octave/older MATLAB code.
        } else if let Some(kind) = open_kind(keyword) {
            blocks.push(OpenBlock {
                kind,
                line: first.line,
            });
        } else if let Some(expected) = close_kind(keyword) {
            let Some(open) = blocks.pop() else {
                return Err(error(first.line, format!("Unexpected '{keyword}'")));
            };
            if expected.is_some_and(|kind| kind != open.kind) {
                return Err(error(
                    first.line,
                    format!("'{keyword}' does not close the block at line {}", open.line),
                ));
            }
            if open.kind == Block::Function && blocks.is_empty() {
                let (name, start, start_line) =
                    local_start.take().expect("local function start exists");
                let end = first.end;
                let mut function_source = blank(&source[..start]);
                function_source.push_str(&source[start..end]);
                function_source.push('\n');
                prepared.local_functions.push(LocalFunction {
                    name,
                    source: function_source,
                    span: SourceSpan {
                        start,
                        end,
                        start_line,
                        end_line: first.line,
                    },
                });
            }
        }
    }
    if let Some(open) = blocks.last() {
        return Err(error(
            open.line,
            "Unclosed block; script local functions require explicit end keywords",
        ));
    }
    let mut output = source.as_bytes().to_vec();
    for function in &prepared.local_functions {
        for byte in &mut output[function.span.start..function.span.end] {
            if !matches!(*byte, b'\n' | b'\r' | b'\t') {
                *byte = b' ';
            }
        }
    }
    prepared.source = String::from_utf8(output).expect("blanking preserves UTF-8");
    Ok(prepared)
}

fn function_name(statement: &Statement, source: &str) -> Result<String> {
    let tokens = &statement.tokens;
    let equals = tokens.iter().position(|token| {
        token.kind == TokenKind::Symbol && &source[token.start..token.end] == "="
    });
    let index = equals.map_or(1, |position| position + 1);
    let token = tokens
        .get(index)
        .ok_or_else(|| error(tokens[0].line, "Missing function name"))?;
    if token.kind != TokenKind::Word {
        return Err(error(token.line, "Unsupported function declaration"));
    }
    Ok(source[token.start..token.end].to_owned())
}

fn open_kind(keyword: &str) -> Option<Block> {
    Some(match keyword {
        "if" => Block::If,
        "for" | "parfor" => Block::For,
        "while" => Block::While,
        "switch" => Block::Switch,
        "try" => Block::Try,
        "unwind_protect" => Block::UnwindProtect,
        "do" => Block::Do,
        "arguments" => Block::Arguments,
        _ => return None,
    })
}

fn close_kind(keyword: &str) -> Option<Option<Block>> {
    Some(match keyword {
        "end" => None,
        "endfunction" => Some(Block::Function),
        "endif" => Some(Block::If),
        "endfor" => Some(Block::For),
        "endwhile" => Some(Block::While),
        "endswitch" => Some(Block::Switch),
        "end_try_catch" => Some(Block::Try),
        "end_unwind_protect" => Some(Block::UnwindProtect),
        "until" => Some(Block::Do),
        _ => return None,
    })
}

fn blank(source: &str) -> String {
    // Only the last prefix line needs column padding. Earlier lines need just
    // their newline characters, avoiding a full copy of large script literals
    // for every private function while keeping all source positions correct.
    let last_line = source.rfind('\n').map_or(0, |index| index + 1);
    let mut padding: String = source[..last_line]
        .bytes()
        .filter(|byte| matches!(byte, b'\n' | b'\r'))
        .map(char::from)
        .collect();
    padding.extend(source[last_line..].bytes().map(|byte| {
        if matches!(byte, b'\r' | b'\t') {
            char::from(byte)
        } else {
            ' '
        }
    }));
    padding
}

fn error(line: usize, message: impl Into<String>) -> PrepareError {
    PrepareError {
        line,
        message: message.into(),
    }
}
