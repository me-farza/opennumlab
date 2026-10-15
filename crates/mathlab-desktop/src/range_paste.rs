// SPDX-License-Identifier: GPL-3.0-or-later
//! CP03 literal range/paste drafts. Parsing does not dispatch an engine write.
//!
//! TSV is row-oriented; the resulting typed components are column-major.
//! Parsing validates the entire selected rectangle before returning any payload.
//! The future engine owner must independently check the complete binding/path,
//! class, shape, storage, selection, budgets and atomic transaction precondition.

use mathlab_engine::value_edit::{EditResult, LeafDescriptor, RangeEditRequest};
use mathlab_engine::{
    DecimalU64, NumericClass, NumericComponents, PageSelection, PageWindow, SourceStorage,
    ValuePageRequest, ValuePath, ValuePathStep,
};
use std::fmt;

const MAX_BACKEND_INDEX: u64 = (1_u64 << 53) - 1;
const MAX_RANK: usize = 32;
const MAX_PATH_DEPTH: usize = 16;

/// Capture this from an accepted typed value and explicit rectangular selection.
/// Starts and higher-axis indices are one-based; no linear indexing is inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteTarget {
    pub path: ValuePath,
    pub class: NumericClass,
    pub source_shape: Vec<DecimalU64>,
    pub source_storage: SourceStorage,
    pub complex: bool,
    pub selection: PageSelection,
}

/// Independent clipboard, cell, scalar and retained-component budgets. Tighten
/// these for a particular UI; they are not permission to grow the wire protocol.
#[derive(Debug, Clone, Copy)]
pub struct PasteLimits {
    pub input_bytes: usize,
    pub cells: usize,
    pub scalar_bytes: usize,
    pub component_bytes: usize,
}

impl Default for PasteLimits {
    fn default() -> Self {
        Self {
            input_bytes: 1024 * 1024,
            cells: 2_000,
            scalar_bytes: 128,
            component_bytes: 2_000 * 2 * 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRangePaste {
    pub target: PasteTarget,
    /// Each component uses row + column * selected_rows, including sparse
    /// destinations. A bounded pasted rectangle is not a dense source copy.
    pub real: NumericComponents,
    pub imag: Option<NumericComponents>,
}

impl ParsedRangePaste {
    /// Consume this exact accepted-target draft into the shared edit codec.
    /// In particular, native floats are encoded there as IEEE bit hex; they
    /// never round-trip through display text or a second decimal parser here.
    /// This conversion validates but does not dispatch or accept a transaction.
    pub fn into_edit_request(self) -> EditResult<RangeEditRequest> {
        let target = self.target;
        RangeEditRequest::from_components(
            target.path,
            PageWindow {
                row_start: target.selection.row_start,
                column_start: target.selection.column_start,
                rows: target.selection.rows,
                columns: target.selection.columns,
                higher_indices: target.selection.higher_indices,
            },
            LeafDescriptor {
                class: target.class,
                source_shape: target.source_shape,
                source_storage: target.source_storage,
                complex: target.complex,
            },
            &self.real,
            self.imag.as_ref(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    EmptyInput,
    InvalidTarget(String),
    Limit {
        resource: &'static str,
        actual: usize,
        maximum: usize,
    },
    Ragged {
        row: usize,
        expected: usize,
        actual: usize,
    },
    ShapeMismatch {
        selected_rows: usize,
        selected_columns: usize,
        pasted_rows: usize,
        pasted_columns: usize,
    },
    InvalidCell {
        row: usize,
        column: usize,
        reason: &'static str,
    },
}

impl fmt::Display for PasteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("The clipboard has no numeric cells."),
            Self::InvalidTarget(reason) => write!(f, "The selected range is invalid: {reason}"),
            Self::Limit {
                resource,
                actual,
                maximum,
            } => write!(
                f,
                "Paste {resource} exceeds its limit ({actual} > {maximum})."
            ),
            Self::Ragged {
                row,
                expected,
                actual,
            } => write!(
                f,
                "Clipboard row {row} has {actual} cells; expected {expected}."
            ),
            Self::ShapeMismatch {
                selected_rows,
                selected_columns,
                pasted_rows,
                pasted_columns,
            } => write!(
                f,
                "Clipboard size {pasted_rows}×{pasted_columns} does not match selected size \
                 {selected_rows}×{selected_columns}."
            ),
            Self::InvalidCell {
                row,
                column,
                reason,
            } => write!(f, "Clipboard cell ({row},{column}): {reason}"),
        }
    }
}

impl std::error::Error for PasteError {}

fn limit(resource: &'static str, actual: usize, maximum: usize) -> Result<(), PasteError> {
    if actual > maximum {
        Err(PasteError::Limit {
            resource,
            actual,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn invalid(reason: impl Into<String>) -> PasteError {
    PasteError::InvalidTarget(reason.into())
}

fn class_width(class: NumericClass) -> usize {
    match class {
        NumericClass::Double | NumericClass::Int64 | NumericClass::Uint64 => 8,
        NumericClass::Single | NumericClass::Int32 | NumericClass::Uint32 => 4,
        NumericClass::Int16 | NumericClass::Uint16 => 2,
        NumericClass::Int8 | NumericClass::Uint8 | NumericClass::Logical => 1,
    }
}

fn validate_target(target: &PasteTarget, limits: PasteLimits) -> Result<usize, PasteError> {
    if limits.input_bytes == 0
        || limits.cells == 0
        || limits.scalar_bytes == 0
        || limits.component_bytes == 0
    {
        return Err(invalid("paste budgets must be positive"));
    }
    if !(2..=MAX_RANK).contains(&target.source_shape.len()) {
        return Err(invalid("source rank must be between two and 32"));
    }
    if target.selection.higher_indices.len() != target.source_shape.len() - 2 {
        return Err(invalid(
            "one explicit selector is required for every higher axis",
        ));
    }
    // Bound the path before cloning it into the existing request validator.
    if target.path.steps.len() > MAX_PATH_DEPTH
        || target.path.root.name.len() > 63
        || target.path.root.context.worker_id.len() > 256
        || target.path.steps.iter().any(|step| match step {
            ValuePathStep::Cell { indices } => indices.len() > MAX_RANK,
            ValuePathStep::Field { indices, name } => indices.len() > MAX_RANK || name.len() > 63,
        })
    {
        return Err(invalid("binding path exceeds the typed-page limits"));
    }
    ValuePageRequest {
        path: target.path.clone(),
        window: PageWindow {
            row_start: target.selection.row_start,
            column_start: target.selection.column_start,
            rows: target.selection.rows,
            columns: target.selection.columns,
            higher_indices: target.selection.higher_indices.clone(),
        },
    }
    .validate()
    .map_err(|error| invalid(error.to_string()))?;

    let mut source_cells = 1_u64;
    for extent in &target.source_shape {
        if extent.get() == 0 || extent.get() > MAX_BACKEND_INDEX {
            return Err(invalid(
                "an editable source must have positive extents below 2^53",
            ));
        }
        source_cells = source_cells
            .checked_mul(extent.get())
            .ok_or_else(|| invalid("source element count overflows u64"))?;
    }
    for (index, extent) in target
        .selection
        .higher_indices
        .iter()
        .zip(&target.source_shape[2..])
    {
        if index.get() == 0 || index.get() > extent.get() {
            return Err(invalid(
                "a higher-axis selector lies outside its source axis",
            ));
        }
    }
    for (start, count, extent) in [
        (
            target.selection.row_start.get(),
            target.selection.rows,
            target.source_shape[0].get(),
        ),
        (
            target.selection.column_start.get(),
            target.selection.columns,
            target.source_shape[1].get(),
        ),
    ] {
        if start == 0
            || count == 0
            || start
                .checked_add(u64::from(count) - 1)
                .is_none_or(|end| end > extent)
        {
            return Err(invalid(
                "paste cannot clip, grow or reshape the selected source",
            ));
        }
    }
    if target.complex && !matches!(target.class, NumericClass::Double | NumericClass::Single) {
        return Err(invalid("only single/double destinations may be complex"));
    }
    if target.source_storage == SourceStorage::Sparse
        && (target.source_shape.len() != 2
            || !matches!(target.class, NumericClass::Double | NumericClass::Logical))
    {
        return Err(invalid(
            "this profile supports sparse 2-D double/logical destinations",
        ));
    }
    let count =
        usize::try_from(u64::from(target.selection.rows) * u64::from(target.selection.columns))
            .map_err(|_| invalid("selected cell count does not fit this platform"))?;
    limit("cells", count, limits.cells)?;
    let bytes = count
        .checked_mul(class_width(target.class))
        .and_then(|bytes| bytes.checked_mul(if target.complex { 2 } else { 1 }))
        .ok_or_else(|| invalid("retained component byte count overflows"))?;
    limit("component bytes", bytes, limits.component_bytes)?;
    Ok(count)
}

#[derive(Clone, Copy)]
struct Cell<'a> {
    text: &'a str,
    row: usize,
    column: usize,
}

impl Cell<'_> {
    fn error(self, reason: &'static str) -> PasteError {
        PasteError::InvalidCell {
            row: self.row,
            column: self.column,
            reason,
        }
    }
}

/// Accept LF or CRLF and at most one final row terminator. Empty fields/rows,
/// quoted CSV, comma separators and bare CR are not silently repaired.
fn tsv_cells(
    input: &str,
    limits: PasteLimits,
) -> Result<(Vec<Cell<'_>>, usize, usize), PasteError> {
    if input.is_empty() {
        return Err(PasteError::EmptyInput);
    }
    let mut lines = input.split('\n').peekable();
    let mut cells = Vec::new();
    let mut rows = 0_usize;
    let mut columns = None;
    while let Some(line) = lines.next() {
        let ended_by_lf = lines.peek().is_some();
        if line.is_empty() && !ended_by_lf && input.ends_with('\n') {
            break;
        }
        let line = if ended_by_lf {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            line
        };
        rows = rows
            .checked_add(1)
            .ok_or_else(|| invalid("row count overflow"))?;
        limit("rows", rows, limits.cells)?;
        let width = line.split('\t').count();
        if let Some(expected) = columns {
            if width != expected {
                return Err(PasteError::Ragged {
                    row: rows,
                    expected,
                    actual: width,
                });
            }
        } else {
            columns = Some(width);
        }
        let count = rows
            .checked_mul(width)
            .ok_or_else(|| invalid("clipboard cell count overflow"))?;
        limit("cells", count, limits.cells)?;
        for (column, text) in line.split('\t').enumerate() {
            let cell = Cell {
                text: text.trim_matches(' '),
                row: rows,
                column: column + 1,
            };
            if cell.text.is_empty() {
                return Err(cell.error("empty cells are not numeric values"));
            }
            if cell.text.contains('\r') {
                return Err(cell.error("bare CR is not a supported row terminator"));
            }
            limit("scalar bytes", cell.text.len(), limits.scalar_bytes)?;
            cells.push(cell);
        }
    }
    Ok((cells, rows, columns.unwrap_or(0)))
}

#[derive(Clone, Copy)]
enum FloatText {
    Nan,
    PositiveInfinity,
    NegativeInfinity,
    Finite { nonzero: bool },
}

fn float_text(text: &str) -> Option<FloatText> {
    match text {
        "NaN" | "+NaN" => return Some(FloatText::Nan),
        "Inf" | "+Inf" => return Some(FloatText::PositiveInfinity),
        "-Inf" => return Some(FloatText::NegativeInfinity),
        _ => {}
    }
    let bytes = text.as_bytes();
    let mut position = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = position;
    let mut nonzero = false;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        nonzero |= bytes[position] != b'0';
        position += 1;
    }
    let mut digits = position - start;
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            nonzero |= bytes[position] != b'0';
            position += 1;
        }
        digits += position - start;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(position), Some(b'e' | b'E')) {
        position += 1;
        if matches!(bytes.get(position), Some(b'+' | b'-')) {
            position += 1;
        }
        let start = position;
        while bytes.get(position).is_some_and(u8::is_ascii_digit) {
            position += 1;
        }
        if position == start {
            return None;
        }
    }
    (position == bytes.len()).then_some(FloatText::Finite { nonzero })
}

macro_rules! float_parser {
    ($name:ident, $ty:ty) => {
        fn $name(text: &str) -> Result<$ty, &'static str> {
            match float_text(text).ok_or("expected a decimal literal, NaN, Inf or -Inf")? {
                FloatText::Nan => Ok(<$ty>::NAN),
                FloatText::PositiveInfinity => Ok(<$ty>::INFINITY),
                FloatText::NegativeInfinity => Ok(<$ty>::NEG_INFINITY),
                FloatText::Finite { nonzero } => {
                    let value = text
                        .parse::<$ty>()
                        .map_err(|_| "invalid floating literal")?;
                    if !value.is_finite() {
                        return Err("finite literal overflows the destination class");
                    }
                    if value == 0.0 && nonzero {
                        return Err("nonzero literal underflows to zero in the destination class");
                    }
                    Ok(value)
                }
            }
        }
    };
}

float_parser!(parse_f64, f64);
float_parser!(parse_f32, f32);

fn integer_text(text: &str, signed: bool) -> bool {
    let digits = if signed {
        text.strip_prefix('+')
            .or_else(|| text.strip_prefix('-'))
            .unwrap_or(text)
    } else {
        text.strip_prefix('+').unwrap_or(text)
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Pure imaginary i/j, signed decimal components and scientific exponents are
/// literals. Calls, arithmetic, names and internal whitespace are never evaluated.
fn complex_parts(text: &str, complex: bool) -> Result<(&str, &str), &'static str> {
    let Some(body) = text.strip_suffix('i').or_else(|| text.strip_suffix('j')) else {
        return Ok((text, "0"));
    };
    if !complex {
        return Err("complex input cannot change a real destination's complex flag");
    }
    let split = body.bytes().enumerate().skip(1).rfind(|&(index, byte)| {
        matches!(byte, b'+' | b'-') && !matches!(body.as_bytes()[index - 1], b'e' | b'E')
    });
    let (real, imaginary) = split.map_or(("0", body), |(index, _)| body.split_at(index));
    let imaginary = match imaginary {
        "" | "+" => "1",
        "-" => "-1",
        text => text,
    };
    Ok((real, imaginary))
}

fn components(
    target: &PasteTarget,
    cells: &[Cell<'_>],
    rows: usize,
    columns: usize,
) -> Result<(NumericComponents, Option<NumericComponents>), PasteError> {
    let count = cells.len();
    let ordered = || {
        (0..columns).flat_map(move |column| (0..rows).map(move |row| cells[row * columns + column]))
    };
    macro_rules! integers {
        ($ty:ty, $variant:ident, $signed:expr) => {{
            let mut values = Vec::with_capacity(count);
            for cell in ordered() {
                if !integer_text(cell.text, $signed) {
                    return Err(cell.error("expected a base-10 integer literal without arithmetic"));
                }
                values.push(cell.text.parse::<$ty>().map_err(|_| {
                    cell.error("integer literal is outside the destination class range")
                })?);
            }
            Ok((NumericComponents::$variant(values), None))
        }};
    }
    macro_rules! floats {
        ($parse:ident, $variant:ident) => {{
            let mut real = Vec::with_capacity(count);
            let mut imag = target.complex.then(|| Vec::with_capacity(count));
            for cell in ordered() {
                let (real_text, imag_text) = complex_parts(cell.text, target.complex)
                    .map_err(|reason| cell.error(reason))?;
                real.push($parse(real_text).map_err(|reason| cell.error(reason))?);
                if let Some(imag) = &mut imag {
                    imag.push($parse(imag_text).map_err(|reason| cell.error(reason))?);
                }
            }
            Ok((
                NumericComponents::$variant(real),
                imag.map(NumericComponents::$variant),
            ))
        }};
    }
    match target.class {
        NumericClass::Double => floats!(parse_f64, Double),
        NumericClass::Single => floats!(parse_f32, Single),
        NumericClass::Int8 => integers!(i8, Int8, true),
        NumericClass::Uint8 => integers!(u8, Uint8, false),
        NumericClass::Int16 => integers!(i16, Int16, true),
        NumericClass::Uint16 => integers!(u16, Uint16, false),
        NumericClass::Int32 => integers!(i32, Int32, true),
        NumericClass::Uint32 => integers!(u32, Uint32, false),
        NumericClass::Int64 => integers!(i64, Int64, true),
        NumericClass::Uint64 => integers!(u64, Uint64, false),
        NumericClass::Logical => {
            let values = ordered()
                .map(|cell| match cell.text {
                    "0" | "false" => Ok(false),
                    "1" | "true" => Ok(true),
                    _ => Err(cell.error("expected 0, 1, false or true")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((NumericComponents::Logical(values), None))
        }
    }
}

/// Parse one explicit rectangle without scalar broadcasting, clipping or class/
/// shape changes. Nothing is dispatched and no destination cell is ever touched.
pub fn parse_range_paste(
    input: &str,
    target: &PasteTarget,
    limits: PasteLimits,
) -> Result<ParsedRangePaste, PasteError> {
    limit("input bytes", input.len(), limits.input_bytes)?;
    let expected_count = validate_target(target, limits)?;
    let (cells, rows, columns) = tsv_cells(input, limits)?;
    if rows != target.selection.rows as usize || columns != target.selection.columns as usize {
        return Err(PasteError::ShapeMismatch {
            selected_rows: target.selection.rows as usize,
            selected_columns: target.selection.columns as usize,
            pasted_rows: rows,
            pasted_columns: columns,
        });
    }
    if cells.len() != expected_count {
        return Err(invalid(
            "clipboard component count does not match its rectangle",
        ));
    }
    let (real, imag) = components(target, &cells, rows, columns)?;
    Ok(ParsedRangePaste {
        target: target.clone(),
        real,
        imag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathlab_engine::value_edit::{ComponentEncoding, EditData};
    use mathlab_engine::{BindingToken, SessionContext};

    fn target_for(class: NumericClass, rows: u32, columns: u32) -> PasteTarget {
        PasteTarget {
            path: ValuePath {
                root: BindingToken {
                    context: SessionContext {
                        worker_id: "range-parser-test-worker".into(),
                        generation: 2,
                        invocation: 9,
                        revision: 9_007_199_254_741_009,
                    },
                    name: "A".into(),
                },
                steps: Vec::new(),
            },
            class,
            source_shape: vec![100.into(), 100.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
            selection: PageSelection {
                row_start: 1.into(),
                column_start: 1.into(),
                rows,
                columns,
                higher_indices: Vec::new(),
            },
        }
    }

    fn parse(input: &str, target: &PasteTarget) -> Result<ParsedRangePaste, PasteError> {
        parse_range_paste(input, target, PasteLimits::default())
    }

    #[test]
    fn uint64_values_and_column_major_order_never_pass_through_floating_point() {
        let target = target_for(NumericClass::Uint64, 2, 2);
        let parsed = parse("9007199254741009\t18446744073709551615\n0\t42", &target).unwrap();
        assert_eq!(parsed.target, target);
        assert_eq!(
            parsed.real,
            NumericComponents::Uint64(vec![9_007_199_254_741_009, 0, u64::MAX, 42])
        );
        assert!(parsed.imag.is_none());
    }

    #[test]
    fn signed_extrema_are_exact_and_integer_overflow_has_a_cell_location() {
        let target = target_for(NumericClass::Int64, 1, 2);
        let parsed = parse("-9223372036854775808\t9223372036854775807", &target).unwrap();
        assert_eq!(
            parsed.real,
            NumericComponents::Int64(vec![i64::MIN, i64::MAX])
        );
        assert!(matches!(
            parse("0\t9223372036854775808", &target),
            Err(PasteError::InvalidCell {
                row: 1,
                column: 2,
                ..
            })
        ));
        assert!(parse(
            "0\t18446744073709551616",
            &target_for(NumericClass::Uint64, 1, 2)
        )
        .is_err());
    }

    #[test]
    fn every_integer_class_uses_its_own_range() {
        for (class, good, bad) in [
            (NumericClass::Int8, "-128", "128"),
            (NumericClass::Uint8, "255", "256"),
            (NumericClass::Int16, "-32768", "32768"),
            (NumericClass::Uint16, "65535", "65536"),
            (NumericClass::Int32, "-2147483648", "2147483648"),
            (NumericClass::Uint32, "4294967295", "4294967296"),
        ] {
            let destination = target_for(class, 1, 1);
            assert_eq!(parse(good, &destination).unwrap().real.len(), 1);
            assert!(parse(bad, &destination).is_err(), "{class:?}");
        }
    }

    #[test]
    fn decimal_integer_spelling_is_explicit_and_expressions_are_rejected() {
        assert_eq!(
            parse(" +00042 ", &target_for(NumericClass::Uint64, 1, 1))
                .unwrap()
                .real,
            NumericComponents::Uint64(vec![42])
        );
        for text in [
            "-0",
            "-1",
            "1e3",
            "1.0",
            "0x10",
            "1_000",
            "1+1",
            "uint64(1)",
        ] {
            assert!(
                parse(text, &target_for(NumericClass::Uint64, 1, 1)).is_err(),
                "{text}"
            );
        }
        for text in ["1+1", "sin(0)", "[1 2]", "0x10", "x", "1;quit", "1 / 2"] {
            assert!(
                parse(text, &target_for(NumericClass::Double, 1, 1)).is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn nd_range_preserves_the_full_binding_path_source_shape_and_plane() {
        let mut target = target_for(NumericClass::Int64, 2, 2);
        target.path.steps.push(ValuePathStep::Field {
            indices: vec![1.into(), 1.into()],
            name: "samples".into(),
        });
        target.source_shape = vec![3.into(), 4.into(), 5.into()];
        target.selection.row_start = 2.into();
        target.selection.column_start = 2.into();
        target.selection.higher_indices = vec![4.into()];
        let parsed = parse("37\t40\n38\t41", &target).unwrap();
        assert_eq!(parsed.target, target);
        assert_eq!(parsed.real, NumericComponents::Int64(vec![37, 38, 40, 41]));
        target.selection.higher_indices = vec![2.into()];
        let other_plane = parse("37\t40\n38\t41", &target).unwrap();
        assert_ne!(other_plane.target, parsed.target);
        assert_eq!(other_plane.target.path.root, parsed.target.path.root);
    }

    #[test]
    fn windows_rows_and_columns_are_rectangular_without_broadcast_or_clipping() {
        let target = target_for(NumericClass::Double, 2, 2);
        let parsed = parse("1\t2\r\n3\t4\r\n", &target).unwrap();
        assert_eq!(
            parsed.real,
            NumericComponents::Double(vec![1.0, 3.0, 2.0, 4.0])
        );
        assert!(matches!(
            parse("1\t2\n3", &target),
            Err(PasteError::Ragged { row: 2, .. })
        ));
        assert!(matches!(
            parse("1", &target),
            Err(PasteError::ShapeMismatch { .. })
        ));
        for input in [
            "1\t\n3\t4",
            "1\t2\n3\t4\n\n",
            "1\t2\r3\t4",
            "1,2\n3,4",
            "\"1\"\t2\n3\t4",
        ] {
            assert!(parse(input, &target).is_err(), "{input:?}");
        }
    }

    #[test]
    fn logical_literals_are_not_truthiness_or_numeric_conversion() {
        let target = target_for(NumericClass::Logical, 2, 2);
        assert_eq!(
            parse("true\t0\nfalse\t1", &target).unwrap().real,
            NumericComponents::Logical(vec![true, false, false, true])
        );
        for input in ["2", "-1", "NaN", "TRUE", "1.0", "true()"] {
            assert!(
                parse(input, &target_for(NumericClass::Logical, 1, 1)).is_err(),
                "{input}"
            );
        }
    }

    #[test]
    fn binary32_rounds_directly_and_floats_preserve_signed_zero_and_special_values() {
        let parsed = parse("0.1\t-0", &target_for(NumericClass::Single, 1, 2)).unwrap();
        match parsed.real {
            NumericComponents::Single(values) => {
                assert_eq!(values[0].to_bits(), 0.1_f32.to_bits());
                assert_eq!(values[1].to_bits(), (-0.0_f32).to_bits());
            }
            _ => panic!("expected native binary32 components"),
        }
        let parsed = parse(
            "NaN\t+Inf\t-Inf\t-0.0",
            &target_for(NumericClass::Double, 1, 4),
        )
        .unwrap();
        match parsed.real {
            NumericComponents::Double(values) => {
                assert!(values[0].is_nan());
                assert_eq!(values[1], f64::INFINITY);
                assert_eq!(values[2], f64::NEG_INFINITY);
                assert_eq!(values[3].to_bits(), (-0.0_f64).to_bits());
            }
            _ => panic!("expected native binary64 components"),
        }
        for (class, text) in [
            (NumericClass::Double, "1e309"),
            (NumericClass::Double, "1e-999"),
            (NumericClass::Single, "1e39"),
            (NumericClass::Single, "1e-99"),
        ] {
            assert!(
                parse(text, &target_for(class, 1, 1)).is_err(),
                "{class:?}: {text}"
            );
        }
        assert!(parse("5e-324", &target_for(NumericClass::Double, 1, 1)).is_ok());
    }

    #[test]
    fn binary32_midpoint_and_just_above_it_are_rounded_once_before_edit_encoding() {
        let parsed = parse(
            "1.000000059604644775390626\t1.000000059604644775390625",
            &target_for(NumericClass::Single, 1, 2),
        )
        .unwrap();
        match &parsed.real {
            NumericComponents::Single(values) => {
                // Independent binary32 oracles: 1 + 2^-24 is the exact
                // midpoint above 1.0. Its even neighbor is 1.0 (3f800000).
                // The first decimal lies strictly above that midpoint; an
                // intermediate f64 would erase this distinction.
                assert_eq!(values[0].to_bits(), 0x3f80_0001);
                assert_eq!(values[1].to_bits(), 0x3f80_0000);
            }
            _ => panic!("expected direct native binary32 parsing"),
        }
        let request = parsed.into_edit_request().unwrap();
        let EditData::Dense {
            encoding,
            real,
            imag,
        } = request.data;
        assert_eq!(encoding, ComponentEncoding::Ieee754Hex);
        assert_eq!(real, ["3f800001", "3f800000"]);
        assert!(imag.is_empty());
    }

    #[test]
    fn shared_edit_conversion_preserves_context_integers_and_native_double_bits() {
        let mut target = target_for(NumericClass::Uint64, 1, 2);
        target.source_shape.push(5.into());
        target.selection.higher_indices.push(4.into());
        let parsed = parse("9007199254741009\t18446744073709551615", &target).unwrap();
        let request = parsed.into_edit_request().unwrap();
        assert_eq!(request.path, target.path);
        assert_eq!(request.expected_leaf.class, target.class);
        assert_eq!(request.expected_leaf.source_shape, target.source_shape);
        assert_eq!(request.expected_leaf.source_storage, target.source_storage);
        assert_eq!(request.expected_leaf.complex, target.complex);
        assert_eq!(
            request.window.higher_indices,
            target.selection.higher_indices
        );
        assert_eq!(request.window.row_start, target.selection.row_start);
        assert_eq!(request.window.column_start, target.selection.column_start);
        assert_eq!(request.window.rows, target.selection.rows);
        assert_eq!(request.window.columns, target.selection.columns);
        let EditData::Dense {
            encoding,
            real,
            imag,
        } = request.data;
        assert_eq!(encoding, ComponentEncoding::Decimal);
        assert_eq!(real, ["9007199254741009", "18446744073709551615"]);
        assert!(imag.is_empty());

        let parsed = parse("-0\tInf", &target_for(NumericClass::Double, 1, 2)).unwrap();
        let request = parsed.into_edit_request().unwrap();
        let EditData::Dense {
            encoding,
            real,
            imag,
        } = request.data;
        assert_eq!(encoding, ComponentEncoding::Ieee754Hex);
        assert_eq!(real, ["8000000000000000", "7ff0000000000000"]);
        assert!(imag.is_empty());
    }

    #[test]
    fn shared_edit_codec_rejects_a_reconstructed_draft_with_the_wrong_payload_class() {
        let mut parsed = parse("1", &target_for(NumericClass::Single, 1, 1)).unwrap();
        parsed.real = NumericComponents::Double(vec![1.0]);
        assert!(parsed.into_edit_request().is_err());
        let mut parsed = parse("1", &target_for(NumericClass::Uint64, 1, 1)).unwrap();
        parsed.real = NumericComponents::Int64(vec![1]);
        assert!(parsed.into_edit_request().is_err());
    }

    #[test]
    fn complex_literals_distinguish_exponent_signs_and_keep_both_components() {
        let mut target = target_for(NumericClass::Double, 1, 4);
        target.complex = true;
        let parsed = parse("1e-2-3e-4i\t-i\t2+0j\t-0-0i", &target).unwrap();
        match (parsed.real, parsed.imag) {
            (NumericComponents::Double(real), Some(NumericComponents::Double(imag))) => {
                assert_eq!(real, [0.01, 0.0, 2.0, -0.0]);
                assert_eq!(imag, [-0.0003, -1.0, 0.0, -0.0]);
                assert!(real[3].is_sign_negative());
                assert!(imag[3].is_sign_negative());
            }
            _ => panic!("expected both native double component vectors"),
        }
        let mut one_complex = target_for(NumericClass::Double, 1, 1);
        one_complex.complex = true;
        for input in ["complex(1,2)", "3+-4i", "1 +2i", "1e+i"] {
            assert!(parse(input, &one_complex).is_err());
        }
        assert!(parse("1+0i", &target_for(NumericClass::Double, 1, 1)).is_err());
        let mut nan_target = target_for(NumericClass::Double, 1, 1);
        nan_target.complex = true;
        let parsed = parse("NaN+NaNi", &nan_target).unwrap();
        assert!(matches!(parsed.real, NumericComponents::Double(values) if values[0].is_nan()));
        assert!(
            matches!(parsed.imag, Some(NumericComponents::Double(values)) if values[0].is_nan())
        );
    }

    #[test]
    fn sparse_destination_retains_storage_without_a_source_sized_buffer() {
        let mut target = target_for(NumericClass::Double, 2, 2);
        target.source_shape = vec![2_000_000_000.into(), 2_000_000_000.into()];
        target.source_storage = SourceStorage::Sparse;
        target.selection.row_start = 1_000_000.into();
        target.selection.column_start = 2_000_000.into();
        let parsed = parse("0\t5\n0\t0", &target).unwrap();
        assert_eq!(parsed.target.source_storage, SourceStorage::Sparse);
        assert_eq!(
            parsed.real,
            NumericComponents::Double(vec![0.0, 0.0, 5.0, 0.0])
        );
        assert_eq!(parsed.real.len(), 4);
    }

    #[test]
    fn input_cells_scalar_and_component_budgets_are_independent() {
        let destination = target_for(NumericClass::Double, 1, 2);
        for (limits, input, resource) in [
            (
                PasteLimits {
                    input_bytes: 2,
                    ..PasteLimits::default()
                },
                "1\t2",
                "input bytes",
            ),
            (
                PasteLimits {
                    cells: 1,
                    ..PasteLimits::default()
                },
                "1\t2",
                "cells",
            ),
            (
                PasteLimits {
                    scalar_bytes: 1,
                    ..PasteLimits::default()
                },
                "12\t2",
                "scalar bytes",
            ),
            (
                PasteLimits {
                    component_bytes: 8,
                    ..PasteLimits::default()
                },
                "1\t2",
                "component bytes",
            ),
        ] {
            assert!(
                matches!(parse_range_paste(input, &destination, limits), Err(PasteError::Limit { resource: actual, .. }) if actual == resource)
            );
        }
        let mut complex = destination;
        complex.complex = true;
        assert!(matches!(
            parse_range_paste(
                "1\t2",
                &complex,
                PasteLimits {
                    component_bytes: 16,
                    ..PasteLimits::default()
                }
            ),
            Err(PasteError::Limit {
                resource: "component bytes",
                ..
            })
        ));
    }

    #[test]
    fn invalid_nd_shape_paths_and_bounds_never_get_silently_repaired() {
        let mut destination = target_for(NumericClass::Double, 1, 1);
        destination.source_shape.push(5.into());
        assert!(parse("1", &destination).is_err());
        destination.selection.higher_indices = vec![6.into()];
        assert!(parse("1", &destination).is_err());
        destination.selection.higher_indices = vec![4.into()];
        assert!(parse("1", &destination).is_ok());
        destination.source_shape[2] = 0.into();
        assert!(parse("1", &destination).is_err());
        destination.source_shape = vec![1.into(), 1.into()];
        destination.selection.higher_indices.clear();
        destination.selection.row_start = 2.into();
        assert!(parse("1", &destination).is_err());
        destination.selection.row_start = 1.into();
        destination.path.root.name = "A;quit".into();
        assert!(parse("1", &destination).is_err());
    }

    #[test]
    fn empty_cells_and_oversized_contexts_do_not_authorize_an_empty_or_partial_write() {
        let mut destination = target_for(NumericClass::Double, 1, 1);
        assert!(matches!(
            parse("", &destination),
            Err(PasteError::EmptyInput)
        ));
        assert!(matches!(
            parse(" ", &destination),
            Err(PasteError::InvalidCell {
                row: 1,
                column: 1,
                ..
            })
        ));
        destination.selection.row_start = u64::MAX.into();
        assert!(parse("1", &destination).is_err());
        destination.selection.row_start = 1.into();
        destination.source_shape = vec![MAX_BACKEND_INDEX.into(), MAX_BACKEND_INDEX.into()];
        assert!(parse("1", &destination).is_err());
        destination.source_shape = vec![1.into(), 1.into()];
        destination.selection.higher_indices = vec![1.into(); MAX_RANK + 1];
        assert!(parse("1", &destination).is_err());
    }
}
