// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked CP03 primitive N-D page codec. The optional typed-page API retains
//! CP02 binding identity; legacy display/edit contracts stay intact.
//!
//! Numeric payloads are column-major. COO coordinates are one-based and local to
//! the clipped page, ordered strictly by (column, row). Sparse zero slots and
//! allocation capacity are not observable through this value-level projection.
//! Floating text rounds to its declared binary32/binary64 type; signed zero is
//! retained, but NaN payload/sign and zero-imaginary storage identity are not
//! promised. Overflow and nonzero lexical underflow-to-zero are rejected.
//! Producers use finite round-trip decimal precision (9 significant digits for
//! single, 17 for double), with explicit NaN/Inf/-Inf tokens. Object properties,
//! custom indexing, strings/chars, tables and missing-value schemas are outside
//! this initial primitive subset. Empty first/second axes retain their extent;
//! an empty higher axis has no selectable plane and is explicitly unsupported.

use crate::{BindingToken, SessionContext};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr, time::Duration};

pub const CAP_TYPED_ND_PAGE: &str = "typed_nd_page.v1";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RANK: usize = 32;
pub const MAX_PATH_DEPTH: usize = 16;
pub const MAX_PAGE_CELLS: usize = 2_000;
pub const MAX_COMPONENT_BYTES: usize = 32;
pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
/// The proposed Octave backend must reject extents/indices >= 2^53 before its
/// double-valued size/index conversion. The wire integer itself retains u64.
pub const MAX_BACKEND_INDEX: u64 = (1_u64 << 53) - 1;

pub type ValuePageResult<T> = Result<T, ValuePageError>;

/// Observed result JSON size and elapsed request/settlement/decode time.
/// This is not a count of console bytes or a native allocator measurement.
#[derive(Debug, Clone, Copy)]
pub struct ValuePageReadMetrics {
    pub result_json_bytes: usize,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValuePageError {
    Limit {
        field: &'static str,
        actual: usize,
        limit: usize,
    },
    Invalid {
        field: String,
        reason: String,
    },
    Mismatch {
        field: &'static str,
    },
    Unsupported {
        field: &'static str,
        reason: String,
    },
    Json(String),
}

impl fmt::Display for ValuePageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit {
                field,
                actual,
                limit,
            } => {
                write!(formatter, "{field} exceeds its limit: {actual} > {limit}")
            }
            Self::Invalid { field, reason } => write!(formatter, "Invalid {field}: {reason}"),
            Self::Mismatch { field } => {
                write!(formatter, "Typed page does not match requested {field}")
            }
            Self::Unsupported { field, reason } => {
                write!(formatter, "Unsupported {field}: {reason}")
            }
            Self::Json(reason) => write!(formatter, "Invalid typed-page JSON: {reason}"),
        }
    }
}

impl std::error::Error for ValuePageError {}

fn invalid(field: impl Into<String>, reason: impl Into<String>) -> ValuePageError {
    ValuePageError::Invalid {
        field: field.into(),
        reason: reason.into(),
    }
}

fn limit(field: &'static str, actual: usize, maximum: usize) -> ValuePageResult<()> {
    if actual > maximum {
        Err(ValuePageError::Limit {
            field,
            actual,
            limit: maximum,
        })
    } else {
        Ok(())
    }
}

/// A canonical unsigned decimal JSON string, never an intermediate f64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecimalU64(u64);

impl DecimalU64 {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn checked_usize(self) -> ValuePageResult<usize> {
        usize::try_from(self.0).map_err(|_| invalid("integer", "value does not fit usize"))
    }

    fn backend(self, field: &'static str, allow_zero: bool) -> ValuePageResult<u64> {
        if !allow_zero && self.0 == 0 {
            return Err(invalid(field, "indices are one-based"));
        }
        if self.0 > MAX_BACKEND_INDEX {
            return Err(ValuePageError::Unsupported {
                field,
                reason: "initial Octave backend requires values below 2^53".into(),
            });
        }
        Ok(self.0)
    }
}

impl From<u64> for DecimalU64 {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Serialize for DecimalU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for DecimalU64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.is_empty()
            || text.len() > 20
            || !text.bytes().all(|byte| byte.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(serde::de::Error::custom(
                "expected canonical unsigned decimal string",
            ));
        }
        text.parse::<u64>()
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

// CP02's public token remains the identity type. Strict wire mirrors prevent
// extra fields from silently disappearing during typed-page deserialization.
fn strict_token<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BindingToken, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Context {
        worker_id: String,
        generation: DecimalU64,
        invocation: DecimalU64,
        revision: DecimalU64,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Token {
        context: Context,
        name: String,
    }
    let token = Token::deserialize(deserializer)?;
    Ok(BindingToken {
        context: SessionContext {
            worker_id: token.context.worker_id,
            generation: token.context.generation.get(),
            invocation: token.context.invocation.get(),
            revision: token.context.revision.get(),
        },
        name: token.name,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ValuePathStep {
    /// Exactly one element of a genuine builtin cell, with full subscripts.
    Cell { indices: Vec<DecimalU64> },
    /// Select one genuine builtin struct element, then its named field.
    Field {
        indices: Vec<DecimalU64>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValuePath {
    #[serde(deserialize_with = "strict_token")]
    pub root: BindingToken,
    pub steps: Vec<ValuePathStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageWindow {
    pub row_start: DecimalU64,
    pub column_start: DecimalU64,
    pub rows: u32,
    pub columns: u32,
    /// Exactly one explicit one-based selector per axis after the first two.
    pub higher_indices: Vec<DecimalU64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValuePageRequest {
    pub path: ValuePath,
    pub window: PageWindow,
}

fn identifier(name: &str, field: &'static str) -> ValuePageResult<()> {
    let mut bytes = name.bytes();
    if name.len() > 63
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(invalid(
            field,
            "expected an ASCII identifier of 1 to 63 bytes",
        ));
    }
    Ok(())
}

fn indices(indices: &[DecimalU64], field: &'static str) -> ValuePageResult<()> {
    if indices.len() < 2 {
        return Err(invalid(
            field,
            "full subscripts require at least two dimensions",
        ));
    }
    limit(field, indices.len(), MAX_RANK)?;
    for index in indices {
        index.backend(field, false)?;
    }
    Ok(())
}

impl ValuePageRequest {
    /// Validate before serializing or sending a worker request. Runtime container
    /// class, rank, bounds and getter-free traversal still require worker checks.
    pub fn validate(&self) -> ValuePageResult<()> {
        identifier(&self.path.root.name, "root.name")?;
        if self.path.root.context.worker_id.is_empty()
            || self.path.root.context.worker_id.len() > 256
        {
            return Err(invalid("root.context.worker_id", "expected 1 to 256 bytes"));
        }
        limit("path.steps", self.path.steps.len(), MAX_PATH_DEPTH)?;
        for step in &self.path.steps {
            match step {
                ValuePathStep::Cell { indices: selection } => indices(selection, "path.indices")?,
                ValuePathStep::Field {
                    indices: selection,
                    name,
                } => {
                    indices(selection, "path.indices")?;
                    identifier(name, "path.field")?;
                }
            }
        }
        self.window.row_start.backend("window.row_start", false)?;
        self.window
            .column_start
            .backend("window.column_start", false)?;
        if self.window.rows == 0 || self.window.columns == 0 {
            return Err(invalid(
                "window",
                "requested rows and columns must be positive",
            ));
        }
        let cells = u64::from(self.window.rows)
            .checked_mul(u64::from(self.window.columns))
            .ok_or_else(|| invalid("window", "requested cell count overflow"))?;
        if cells > MAX_PAGE_CELLS as u64 {
            return Err(ValuePageError::Limit {
                field: "window.cells",
                actual: usize::try_from(cells).unwrap_or(usize::MAX),
                limit: MAX_PAGE_CELLS,
            });
        }
        limit(
            "window.higher_indices",
            self.window.higher_indices.len(),
            MAX_RANK - 2,
        )?;
        for index in &self.window.higher_indices {
            index.backend("window.higher_indices", false)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumericClass {
    Double,
    Single,
    Int8,
    Uint8,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
    Uint64,
    Logical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStorage {
    Dense,
    Sparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSelection {
    pub row_start: DecimalU64,
    pub column_start: DecimalU64,
    pub rows: u32,
    pub columns: u32,
    pub higher_indices: Vec<DecimalU64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WirePageData {
    Dense {
        real: Vec<String>,
        imag: Vec<String>,
    },
    SparseCoo {
        rows: Vec<u32>,
        columns: Vec<u32>,
        real: Vec<String>,
        imag: Vec<String>,
    },
}

/// Untrusted DTO. Use decode_json for the byte limit before serde allocation,
/// and decode/validate before exposing values. There is no legacy fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireValuePage {
    pub schema_version: u32,
    #[serde(deserialize_with = "strict_token")]
    pub token: BindingToken,
    pub path: Vec<ValuePathStep>,
    pub class: NumericClass,
    pub source_shape: Vec<DecimalU64>,
    pub source_storage: SourceStorage,
    pub complex: bool,
    pub selection: PageSelection,
    pub data: WirePageData,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumericComponents {
    Double(Vec<f64>),
    Single(Vec<f32>),
    Int8(Vec<i8>),
    Uint8(Vec<u8>),
    Int16(Vec<i16>),
    Uint16(Vec<u16>),
    Int32(Vec<i32>),
    Uint32(Vec<u32>),
    Int64(Vec<i64>),
    Uint64(Vec<u64>),
    Logical(Vec<bool>),
}

impl NumericComponents {
    pub fn len(&self) -> usize {
        match self {
            Self::Double(v) => v.len(),
            Self::Single(v) => v.len(),
            Self::Int8(v) => v.len(),
            Self::Uint8(v) => v.len(),
            Self::Int16(v) => v.len(),
            Self::Uint16(v) => v.len(),
            Self::Int32(v) => v.len(),
            Self::Uint32(v) => v.len(),
            Self::Int64(v) => v.len(),
            Self::Uint64(v) => v.len(),
            Self::Logical(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn zero_at(&self, index: usize) -> bool {
        match self {
            Self::Double(v) => v[index] == 0.0,
            Self::Single(v) => v[index] == 0.0,
            Self::Int8(v) => v[index] == 0,
            Self::Uint8(v) => v[index] == 0,
            Self::Int16(v) => v[index] == 0,
            Self::Uint16(v) => v[index] == 0,
            Self::Int32(v) => v[index] == 0,
            Self::Uint32(v) => v[index] == 0,
            Self::Int64(v) => v[index] == 0,
            Self::Uint64(v) => v[index] == 0,
            Self::Logical(v) => !v[index],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageData {
    Dense {
        real: NumericComponents,
        imag: Option<NumericComponents>,
    },
    SparseCoo {
        rows: Vec<u32>,
        columns: Vec<u32>,
        real: NumericComponents,
        imag: Option<NumericComponents>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValuePage {
    pub token: BindingToken,
    pub path: Vec<ValuePathStep>,
    pub class: NumericClass,
    pub source_shape: Vec<DecimalU64>,
    pub source_storage: SourceStorage,
    pub complex: bool,
    pub selection: PageSelection,
    pub data: PageData,
}

fn shape_product(shape: &[DecimalU64]) -> ValuePageResult<u64> {
    // An ultimately empty shape must not overflow on its preceding axes.
    if shape.iter().any(|extent| extent.get() == 0) {
        return Ok(0);
    }
    shape.iter().try_fold(1_u64, |product, extent| {
        product
            .checked_mul(extent.get())
            .ok_or_else(|| invalid("source_shape", "element count overflow"))
    })
}

fn clipped(
    start: DecimalU64,
    requested: u32,
    extent: u64,
    field: &'static str,
) -> ValuePageResult<u32> {
    if start.get() > extent.max(1) {
        return Err(invalid(field, "page begins outside the source extent"));
    }
    let remaining = if extent == 0 {
        0
    } else {
        extent - start.get() + 1
    };
    Ok(u64::from(requested).min(remaining) as u32)
}

impl WireValuePage {
    fn envelope(&self, request: &ValuePageRequest) -> ValuePageResult<usize> {
        request.validate()?;
        if self.schema_version != SCHEMA_VERSION {
            return Err(invalid("schema_version", "only version 1 is supported"));
        }
        if self.token != request.path.root {
            return Err(ValuePageError::Mismatch {
                field: "root token",
            });
        }
        if self.path != request.path.steps {
            return Err(ValuePageError::Mismatch {
                field: "structured path",
            });
        }
        if self.source_shape.len() < 2 {
            return Err(invalid("source_shape", "rank must be at least two"));
        }
        limit("source_shape.rank", self.source_shape.len(), MAX_RANK)?;
        for extent in &self.source_shape {
            extent.backend("source_shape", true)?;
        }
        shape_product(&self.source_shape)?;
        let higher = &request.window.higher_indices;
        if higher.len() != self.source_shape.len() - 2 {
            return Err(invalid(
                "window.higher_indices",
                "one selector is required for each higher axis",
            ));
        }
        for (index, extent) in higher.iter().zip(&self.source_shape[2..]) {
            if extent.get() == 0 {
                return Err(ValuePageError::Unsupported {
                    field: "window.higher_indices",
                    reason: "an empty higher axis has no selectable positive plane in v1".into(),
                });
            }
            if index.get() > extent.get() {
                return Err(invalid(
                    "window.higher_indices",
                    "selector lies outside its axis",
                ));
            }
        }
        let expected = PageSelection {
            row_start: request.window.row_start,
            column_start: request.window.column_start,
            rows: clipped(
                request.window.row_start,
                request.window.rows,
                self.source_shape[0].get(),
                "window.row_start",
            )?,
            columns: clipped(
                request.window.column_start,
                request.window.columns,
                self.source_shape[1].get(),
                "window.column_start",
            )?,
            higher_indices: higher.clone(),
        };
        if self.selection != expected {
            return Err(ValuePageError::Mismatch {
                field: "clipped selection",
            });
        }
        if self.complex && !matches!(self.class, NumericClass::Double | NumericClass::Single) {
            return Err(invalid("complex", "only floating classes may be complex"));
        }
        if self.source_storage == SourceStorage::Sparse
            && (self.source_shape.len() != 2
                || !matches!(self.class, NumericClass::Double | NumericClass::Logical))
        {
            return Err(ValuePageError::Unsupported {
                field: "source_storage",
                reason: "initial sparse pages support rank-two double or logical values".into(),
            });
        }
        let count = u64::from(expected.rows)
            .checked_mul(u64::from(expected.columns))
            .ok_or_else(|| invalid("selection", "cell count overflow"))?;
        usize::try_from(count).map_err(|_| invalid("selection", "cell count does not fit usize"))
    }

    /// A full typed decode also serves as validation; allocations are bounded
    /// by the previously checked request and component limits.
    pub fn validate(&self, request: &ValuePageRequest) -> ValuePageResult<()> {
        self.decode(request).map(|_| ())
    }

    pub fn decode(&self, request: &ValuePageRequest) -> ValuePageResult<ValuePage> {
        let count = self.envelope(request)?;
        let data = match &self.data {
            WirePageData::Dense { real, imag } => {
                if self.source_storage != SourceStorage::Dense {
                    return Err(ValuePageError::Mismatch {
                        field: "storage/data kind",
                    });
                }
                component_counts(real, imag, count, self.complex)?;
                PageData::Dense {
                    real: components(self.class, real, "data.real")?,
                    imag: if self.complex {
                        Some(components(self.class, imag, "data.imag")?)
                    } else {
                        None
                    },
                }
            }
            WirePageData::SparseCoo {
                rows,
                columns,
                real,
                imag,
            } => {
                if self.source_storage != SourceStorage::Sparse {
                    return Err(ValuePageError::Mismatch {
                        field: "storage/data kind",
                    });
                }
                limit("data.sparse_entries", rows.len(), count)?;
                if columns.len() != rows.len() {
                    return Err(invalid("data.columns", "coordinate counts differ"));
                }
                component_counts(real, imag, rows.len(), self.complex)?;
                let mut previous = None;
                for (offset, (&row, &column)) in rows.iter().zip(columns).enumerate() {
                    if row == 0
                        || column == 0
                        || row > self.selection.rows
                        || column > self.selection.columns
                    {
                        return Err(invalid(
                            format!("data.coordinates[{offset}]"),
                            "coordinate lies outside the clipped page",
                        ));
                    }
                    if previous.is_some_and(|last| (column, row) <= last) {
                        return Err(invalid(
                            format!("data.coordinates[{offset}]"),
                            "coordinates must be unique and ordered by (column,row)",
                        ));
                    }
                    previous = Some((column, row));
                }
                let real = components(self.class, real, "data.real")?;
                let imag = if self.complex {
                    Some(components(self.class, imag, "data.imag")?)
                } else {
                    None
                };
                for index in 0..rows.len() {
                    if real.zero_at(index)
                        && imag.as_ref().is_none_or(|values| values.zero_at(index))
                    {
                        return Err(invalid(
                            format!("data.real[{index}]"),
                            "explicit sparse zero entries are not supported",
                        ));
                    }
                }
                PageData::SparseCoo {
                    rows: rows.clone(),
                    columns: columns.clone(),
                    real,
                    imag,
                }
            }
        };
        Ok(ValuePage {
            token: self.token.clone(),
            path: self.path.clone(),
            class: self.class,
            source_shape: self.source_shape.clone(),
            source_storage: self.source_storage,
            complex: self.complex,
            selection: self.selection.clone(),
            data,
        })
    }
}

fn component_counts(
    real: &[String],
    imag: &[String],
    count: usize,
    complex: bool,
) -> ValuePageResult<()> {
    if real.len() != count {
        return Err(invalid(
            "data.real",
            "component count differs from page/coordinate count",
        ));
    }
    if imag.len() != if complex { count } else { 0 } {
        return Err(invalid(
            "data.imag",
            "imaginary component count does not match the complex flag",
        ));
    }
    for (name, values) in [("data.real", real), ("data.imag", imag)] {
        for (index, token) in values.iter().enumerate() {
            limit("component token bytes", token.len(), MAX_COMPONENT_BYTES)?;
            if !token.is_ascii() {
                return Err(invalid(
                    format!("{name}[{index}]"),
                    "component tokens must be ASCII",
                ));
            }
        }
    }
    Ok(())
}

fn canonical_integer(token: &str, signed: bool) -> bool {
    let digits = if let Some(rest) = token.strip_prefix('-') {
        if !signed || rest == "0" {
            return false;
        }
        rest
    } else {
        token
    };
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits.len() == 1 || !digits.starts_with('0'))
}

fn integers<T: FromStr>(
    tokens: &[String],
    signed: bool,
    field: &'static str,
) -> ValuePageResult<Vec<T>> {
    tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            if !canonical_integer(token, signed) {
                return Err(invalid(
                    format!("{field}[{index}]"),
                    "expected canonical decimal integer",
                ));
            }
            token.parse().map_err(|_| {
                invalid(
                    format!("{field}[{index}]"),
                    "integer is outside its declared class",
                )
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
enum FloatingText {
    Nan,
    PositiveInfinity,
    NegativeInfinity,
    Finite { nonzero: bool },
}

fn floating_text(token: &str) -> Option<FloatingText> {
    match token {
        "NaN" => return Some(FloatingText::Nan),
        "Inf" => return Some(FloatingText::PositiveInfinity),
        "-Inf" => return Some(FloatingText::NegativeInfinity),
        _ => {}
    }
    // Finite grammar: -?(0|[1-9][0-9]*)(.[0-9]+)?([eE][+-]?[0-9]+)?
    // Exponent zero padding is allowed because sprintf emits e-08, for example.
    let bytes = token.as_bytes();
    let mut position = usize::from(bytes.first() == Some(&b'-'));
    let begin = position;
    while position < bytes.len() && bytes[position].is_ascii_digit() {
        position += 1;
    }
    if position == begin || (position - begin > 1 && bytes[begin] == b'0') {
        return None;
    }
    let mut nonzero = bytes[begin..position].iter().any(|byte| *byte != b'0');
    if bytes.get(position) == Some(&b'.') {
        position += 1;
        let begin = position;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            position += 1;
        }
        if position == begin {
            return None;
        }
        nonzero |= bytes[begin..position].iter().any(|byte| *byte != b'0');
    }
    if matches!(bytes.get(position), Some(b'e' | b'E')) {
        position += 1;
        if matches!(bytes.get(position), Some(b'+' | b'-')) {
            position += 1;
        }
        let begin = position;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            position += 1;
        }
        if position == begin {
            return None;
        }
    }
    (position == bytes.len()).then_some(FloatingText::Finite { nonzero })
}

fn components(
    class: NumericClass,
    tokens: &[String],
    field: &'static str,
) -> ValuePageResult<NumericComponents> {
    macro_rules! floats {
        ($ty:ty, $variant:ident) => {{
            let values = tokens
                .iter()
                .enumerate()
                .map(|(index, token)| {
                    let location = || format!("{field}[{index}]");
                    match floating_text(token).ok_or_else(|| {
                        invalid(location(), "expected finite decimal text, NaN, Inf or -Inf")
                    })? {
                        FloatingText::Nan => Ok(<$ty>::NAN),
                        FloatingText::PositiveInfinity => Ok(<$ty>::INFINITY),
                        FloatingText::NegativeInfinity => Ok(<$ty>::NEG_INFINITY),
                        FloatingText::Finite { nonzero } => {
                            let value = token
                                .parse::<$ty>()
                                .map_err(|_| invalid(location(), "invalid floating value"))?;
                            if !value.is_finite() {
                                return Err(invalid(
                                    location(),
                                    "finite token overflows its declared class",
                                ));
                            }
                            if value == 0.0 && nonzero {
                                return Err(invalid(
                                    location(),
                                    "nonzero token underflows to zero",
                                ));
                            }
                            Ok(value)
                        }
                    }
                })
                .collect::<ValuePageResult<Vec<$ty>>>()?;
            Ok(NumericComponents::$variant(values))
        }};
    }
    match class {
        NumericClass::Double => floats!(f64, Double),
        NumericClass::Single => floats!(f32, Single),
        NumericClass::Int8 => Ok(NumericComponents::Int8(integers(tokens, true, field)?)),
        NumericClass::Uint8 => Ok(NumericComponents::Uint8(integers(tokens, false, field)?)),
        NumericClass::Int16 => Ok(NumericComponents::Int16(integers(tokens, true, field)?)),
        NumericClass::Uint16 => Ok(NumericComponents::Uint16(integers(tokens, false, field)?)),
        NumericClass::Int32 => Ok(NumericComponents::Int32(integers(tokens, true, field)?)),
        NumericClass::Uint32 => Ok(NumericComponents::Uint32(integers(tokens, false, field)?)),
        NumericClass::Int64 => Ok(NumericComponents::Int64(integers(tokens, true, field)?)),
        NumericClass::Uint64 => Ok(NumericComponents::Uint64(integers(tokens, false, field)?)),
        NumericClass::Logical => Ok(NumericComponents::Logical(
            tokens
                .iter()
                .enumerate()
                .map(|(index, token)| match token.as_str() {
                    "0" => Ok(false),
                    "1" => Ok(true),
                    _ => Err(invalid(
                        format!("{field}[{index}]"),
                        "logical tokens must be 0 or 1",
                    )),
                })
                .collect::<ValuePageResult<Vec<_>>>()?,
        )),
    }
}

/// Decode only a typed-page result object, after the ordinary response identity
/// has been checked. The ordinary response reader must also apply this operation's
/// byte limit before materializing the enclosing JSON response.
pub fn decode_json(bytes: &[u8], request: &ValuePageRequest) -> ValuePageResult<ValuePage> {
    limit("typed-page payload bytes", bytes.len(), MAX_PAYLOAD_BYTES)?;
    request.validate()?;
    let wire: WireValuePage =
        serde_json::from_slice(bytes).map_err(|error| ValuePageError::Json(error.to_string()))?;
    wire.decode(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn decimals(values: &[u64]) -> Vec<DecimalU64> {
        values.iter().copied().map(DecimalU64::new).collect()
    }

    fn token() -> BindingToken {
        BindingToken {
            context: SessionContext {
                worker_id: "worker-typed-test".into(),
                generation: 2,
                invocation: 3,
                revision: u64::MAX - 1,
            },
            name: "A".into(),
        }
    }

    fn request(rows: u32, columns: u32, higher: &[u64]) -> ValuePageRequest {
        ValuePageRequest {
            path: ValuePath {
                root: token(),
                steps: vec![],
            },
            window: PageWindow {
                row_start: 1.into(),
                column_start: 1.into(),
                rows,
                columns,
                higher_indices: decimals(higher),
            },
        }
    }

    fn dense(
        request: &ValuePageRequest,
        shape: &[u64],
        class: NumericClass,
        real: &[&str],
    ) -> WireValuePage {
        WireValuePage {
            schema_version: SCHEMA_VERSION,
            token: request.path.root.clone(),
            path: request.path.steps.clone(),
            class,
            source_shape: decimals(shape),
            source_storage: SourceStorage::Dense,
            complex: false,
            selection: PageSelection {
                row_start: request.window.row_start,
                column_start: request.window.column_start,
                rows: request.window.rows,
                columns: request.window.columns,
                higher_indices: request.window.higher_indices.clone(),
            },
            data: WirePageData::Dense {
                real: real.iter().map(|text| (*text).to_owned()).collect(),
                imag: vec![],
            },
        }
    }

    fn real(page: ValuePage) -> NumericComponents {
        match page.data {
            PageData::Dense { real, imag: None } => real,
            other => panic!("expected real dense components, got {other:?}"),
        }
    }

    fn decode_value(value: &Value, request: &ValuePageRequest) -> ValuePageResult<ValuePage> {
        decode_json(&serde_json::to_vec(value).unwrap(), request)
    }

    #[test]
    fn decimal_u64_wire_is_lossless_and_rejects_noncanonical_or_numeric_json() {
        for value in [0, 1, 9_007_199_254_741_009, u64::MAX] {
            let encoded = serde_json::to_string(&DecimalU64::new(value)).unwrap();
            assert_eq!(encoded, format!("\"{value}\""));
            assert_eq!(
                serde_json::from_str::<DecimalU64>(&encoded).unwrap().get(),
                value
            );
        }
        for invalid in [
            json!(""),
            json!("00"),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!("1.0"),
            json!("1e3"),
            json!(" 1"),
            json!("1\n"),
            json!("١"),
            json!("18446744073709551616"),
            json!(1),
            json!(1.0),
            Value::Null,
        ] {
            assert!(
                serde_json::from_value::<DecimalU64>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn request_validation_bounds_the_work_before_wire_encoding() {
        request(40, 50, &[]).validate().unwrap();
        let mut invalid_requests = vec![];
        let mut current = request(1, 1, &[]);
        current.window.rows = 0;
        invalid_requests.push(current);
        let mut current = request(1, 1, &[]);
        current.window.column_start = 0.into();
        invalid_requests.push(current);
        invalid_requests.push(request(u32::MAX, u32::MAX, &[]));
        invalid_requests.push(request(1, 1, &[1; MAX_RANK - 1]));
        let mut current = request(1, 1, &[]);
        current.window.row_start = (MAX_BACKEND_INDEX + 1).into();
        invalid_requests.push(current);
        let mut current = request(1, 1, &[]);
        current.path.steps = vec![
            ValuePathStep::Cell {
                indices: decimals(&[1, 1])
            };
            MAX_PATH_DEPTH + 1
        ];
        invalid_requests.push(current);
        let mut current = request(1, 1, &[]);
        current.path.steps = vec![ValuePathStep::Cell {
            indices: decimals(&[1]),
        }];
        invalid_requests.push(current);
        let mut current = request(1, 1, &[]);
        current.path.steps = vec![ValuePathStep::Field {
            indices: decimals(&[1, 1]),
            name: "value;run()".into(),
        }];
        invalid_requests.push(current);
        let mut current = request(1, 1, &[]);
        current.path.root.name = "A(1)".into();
        invalid_requests.push(current);
        for current in invalid_requests {
            assert!(current.validate().is_err(), "{current:?}");
        }
    }

    #[test]
    fn selected_nd_plane_keeps_source_shape_and_column_major_values() {
        let request = request(3, 4, &[4]);
        let values = (37..=48)
            .map(|number| number.to_string())
            .collect::<Vec<_>>();
        let refs = values.iter().map(String::as_str).collect::<Vec<_>>();
        let wire = dense(&request, &[3, 4, 5], NumericClass::Double, &refs);
        let page = decode_json(&serde_json::to_vec(&wire).unwrap(), &request).unwrap();
        assert_eq!(page.source_shape, decimals(&[3, 4, 5]));
        assert_eq!(page.selection.higher_indices, decimals(&[4]));
        assert_eq!((page.selection.rows, page.selection.columns), (3, 4));
        assert_eq!(
            real(page),
            NumericComponents::Double((37..=48).map(f64::from).collect())
        );
    }

    #[test]
    fn clipped_page_must_match_the_exact_requested_window_and_plane() {
        let mut request = request(5, 5, &[4]);
        request.window.row_start = 2.into();
        request.window.column_start = 3.into();
        let mut wire = dense(
            &request,
            &[3, 4, 5],
            NumericClass::Double,
            &["44", "45", "47", "48"],
        );
        wire.selection.rows = 2;
        wire.selection.columns = 2;
        assert_eq!(
            real(wire.decode(&request).unwrap()),
            NumericComponents::Double(vec![44., 45., 47., 48.])
        );
        for (pointer, replacement) in [
            ("/selection/rows", json!(3)),
            ("/selection/columns", json!(1)),
            ("/selection/row_start", json!("1")),
            ("/selection/higher_indices/0", json!("3")),
            ("/data/real", json!(["44", "45", "47"])),
        ] {
            let mut changed = serde_json::to_value(&wire).unwrap();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(decode_value(&changed, &request).is_err(), "{pointer}");
        }
        request.window.higher_indices[0] = 6.into();
        wire.selection.higher_indices = request.window.higher_indices.clone();
        assert!(wire.decode(&request).is_err());
    }

    #[test]
    fn every_integer_class_and_logicals_decode_without_a_float_intermediate() {
        let cases = [
            (
                NumericClass::Int8,
                ["-128", "127"],
                NumericComponents::Int8(vec![i8::MIN, i8::MAX]),
            ),
            (
                NumericClass::Uint8,
                ["0", "255"],
                NumericComponents::Uint8(vec![0, u8::MAX]),
            ),
            (
                NumericClass::Int16,
                ["-32768", "32767"],
                NumericComponents::Int16(vec![i16::MIN, i16::MAX]),
            ),
            (
                NumericClass::Uint16,
                ["0", "65535"],
                NumericComponents::Uint16(vec![0, u16::MAX]),
            ),
            (
                NumericClass::Int32,
                ["-2147483648", "2147483647"],
                NumericComponents::Int32(vec![i32::MIN, i32::MAX]),
            ),
            (
                NumericClass::Uint32,
                ["0", "4294967295"],
                NumericComponents::Uint32(vec![0, u32::MAX]),
            ),
            (
                NumericClass::Int64,
                ["-9223372036854775808", "9223372036854775807"],
                NumericComponents::Int64(vec![i64::MIN, i64::MAX]),
            ),
            (
                NumericClass::Uint64,
                ["9007199254741009", "18446744073709551615"],
                NumericComponents::Uint64(vec![9_007_199_254_741_009, u64::MAX]),
            ),
            (
                NumericClass::Logical,
                ["0", "1"],
                NumericComponents::Logical(vec![false, true]),
            ),
        ];
        for (class, values, expected) in cases {
            let request = request(1, 2, &[]);
            assert_eq!(
                real(
                    dense(&request, &[1, 2], class, &values)
                        .decode(&request)
                        .unwrap()
                ),
                expected
            );
        }
    }

    #[test]
    fn malformed_integer_spellings_and_ranges_are_rejected() {
        for (class, token) in [
            (NumericClass::Uint64, "18446744073709551616"),
            (NumericClass::Int64, "-9223372036854775809"),
            (NumericClass::Int64, "9223372036854775808"),
            (NumericClass::Int8, "128"),
            (NumericClass::Uint8, "256"),
            (NumericClass::Uint16, "-1"),
            (NumericClass::Int32, "+1"),
            (NumericClass::Int32, "01"),
            (NumericClass::Int32, "-0"),
            (NumericClass::Int32, "1e0"),
            (NumericClass::Logical, "true"),
            (NumericClass::Logical, "2"),
        ] {
            let request = request(1, 1, &[]);
            assert!(
                dense(&request, &[1, 1], class, &[token])
                    .decode(&request)
                    .is_err(),
                "{class:?}: {token}"
            );
        }
    }

    #[test]
    fn finite_float_round_trips_nonfinite_tokens_subnormals_and_signed_zero_are_explicit() {
        let single_request = request(1, 7, &[]);
        let wire = dense(
            &single_request,
            &[1, 7],
            NumericClass::Single,
            &[
                "-0",
                "0",
                "NaN",
                "Inf",
                "-Inf",
                "0.100000001",
                "1.40129846e-45",
            ],
        );
        let NumericComponents::Single(values) = real(wire.decode(&single_request).unwrap()) else {
            panic!("single type lost");
        };
        assert_eq!(values[0].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(values[1].to_bits(), 0.0_f32.to_bits());
        assert!(values[2].is_nan());
        assert_eq!(values[3], f32::INFINITY);
        assert_eq!(values[4], f32::NEG_INFINITY);
        assert_eq!(values[5].to_bits(), 0.1_f32.to_bits());
        assert_eq!(values[6].to_bits(), 1);
        let request = request(1, 4, &[]);
        let wire = dense(
            &request,
            &[1, 4],
            NumericClass::Double,
            &[
                "-0e-300",
                "0e+300",
                "4.9406564584124654e-324",
                "1.7976931348623157e+308",
            ],
        );
        let NumericComponents::Double(values) = real(wire.decode(&request).unwrap()) else {
            panic!("double type lost");
        };
        assert_eq!(values[0].to_bits(), (-0.0_f64).to_bits());
        assert_eq!(values[1].to_bits(), 0.0_f64.to_bits());
        assert_eq!(values[2].to_bits(), 1);
        assert_eq!(values[3], f64::MAX);
    }

    #[test]
    fn finite_float_grammar_overflow_and_nonzero_underflow_cannot_silently_pass() {
        for token in [
            "nan",
            "+Inf",
            "-NaN",
            " 1",
            "1 ",
            "+1",
            "01",
            "1.",
            ".5",
            "0x1p0",
            "1e",
            "1e+-2",
            "1e999",
            "1e-999",
            "-0.0001e-999",
        ] {
            let request = request(1, 1, &[]);
            assert!(
                dense(&request, &[1, 1], NumericClass::Double, &[token])
                    .decode(&request)
                    .is_err(),
                "{token}"
            );
        }
        for token in ["3.5e38", "1e-46"] {
            let request = request(1, 1, &[]);
            assert!(
                dense(&request, &[1, 1], NumericClass::Single, &[token])
                    .decode(&request)
                    .is_err(),
                "{token}"
            );
        }
    }

    #[test]
    fn complex_components_preserve_the_declared_float_class_and_column_order() {
        for class in [NumericClass::Double, NumericClass::Single] {
            let request = request(2, 2, &[]);
            let mut wire = dense(&request, &[2, 2], class, &["1", "0", "-2", "0.5"]);
            wire.complex = true;
            let WirePageData::Dense { imag, .. } = &mut wire.data else {
                unreachable!()
            };
            *imag = ["3", "-4", "0", "0.25"].map(str::to_owned).to_vec();
            let page = wire.decode(&request).unwrap();
            assert!(page.complex);
            match (class, page.data) {
                (
                    NumericClass::Double,
                    PageData::Dense {
                        real: NumericComponents::Double(real),
                        imag: Some(NumericComponents::Double(imag)),
                    },
                ) => {
                    assert_eq!(real, vec![1., 0., -2., 0.5]);
                    assert_eq!(imag, vec![3., -4., 0., 0.25]);
                }
                (
                    NumericClass::Single,
                    PageData::Dense {
                        real: NumericComponents::Single(real),
                        imag: Some(NumericComponents::Single(imag)),
                    },
                ) => {
                    assert_eq!(real, vec![1., 0., -2., 0.5]);
                    assert_eq!(imag, vec![3., -4., 0., 0.25]);
                }
                other => panic!("complex class was changed: {other:?}"),
            }
        }
    }

    fn sparse(request: &ValuePageRequest) -> WireValuePage {
        let mut wire = dense(request, &[3, 4], NumericClass::Double, &[]);
        wire.source_storage = SourceStorage::Sparse;
        wire.complex = true;
        wire.data = WirePageData::SparseCoo {
            rows: vec![1, 3],
            columns: vec![2, 4],
            real: vec!["2".into(), "-1".into()],
            imag: vec!["3".into(), "4".into()],
        };
        wire
    }

    #[test]
    fn sparse_coordinates_are_local_to_the_page_and_implicit_zeros_are_not_densified() {
        let mut request = request(2, 3, &[]);
        request.window.row_start = 2.into();
        request.window.column_start = 2.into();
        let mut wire = sparse(&request);
        wire.data = WirePageData::SparseCoo {
            rows: vec![2],
            columns: vec![3],
            real: vec!["-1".into()],
            imag: vec!["4".into()],
        };
        let page = wire.decode(&request).unwrap();
        assert_eq!(page.source_shape, decimals(&[3, 4]));
        match page.data {
            PageData::SparseCoo {
                rows,
                columns,
                real,
                imag,
            } => {
                assert_eq!(rows, vec![2]);
                assert_eq!(columns, vec![3]);
                assert_eq!(real, NumericComponents::Double(vec![-1.]));
                assert_eq!(imag, Some(NumericComponents::Double(vec![4.])));
            }
            _ => panic!("sparse page was densified"),
        }
        wire.data = WirePageData::SparseCoo {
            rows: vec![],
            columns: vec![],
            real: vec![],
            imag: vec![],
        };
        let page = wire.decode(&request).unwrap();
        let PageData::SparseCoo { real, imag, .. } = page.data else {
            panic!("zero sparse page was densified");
        };
        assert!(real.is_empty());
        assert!(imag.unwrap().is_empty());
    }

    #[test]
    fn sparse_logical_values_are_typed_and_explicit_zero_entries_are_rejected() {
        let request = request(3, 4, &[]);
        let mut wire = sparse(&request);
        wire.class = NumericClass::Logical;
        wire.complex = false;
        wire.data = WirePageData::SparseCoo {
            rows: vec![1, 3],
            columns: vec![2, 4],
            real: vec!["1".into(), "1".into()],
            imag: vec![],
        };
        let page = wire.decode(&request).unwrap();
        let PageData::SparseCoo { real, imag, .. } = page.data else {
            panic!("logical sparse page lost");
        };
        assert_eq!(real, NumericComponents::Logical(vec![true, true]));
        assert_eq!(imag, None);
        let WirePageData::SparseCoo { real, .. } = &mut wire.data else {
            unreachable!()
        };
        real[0] = "0".into();
        assert!(wire.decode(&request).is_err());
        let mut wire = sparse(&request);
        let WirePageData::SparseCoo { real, imag, .. } = &mut wire.data else {
            unreachable!()
        };
        real[0] = "-0".into();
        imag[0] = "0".into();
        assert!(wire.decode(&request).is_err());
        let WirePageData::SparseCoo { imag, .. } = &mut wire.data else {
            unreachable!()
        };
        imag[0] = "NaN".into();
        assert!(
            wire.decode(&request).is_ok(),
            "NaN is a nonzero sparse value observation"
        );
    }

    #[test]
    fn malformed_sparse_layouts_classes_and_component_lengths_are_rejected() {
        let request = request(3, 4, &[]);
        let wire = sparse(&request);
        wire.validate(&request).unwrap();
        for (pointer, replacement) in [
            ("/data/rows", json!([1])),
            ("/data/columns", json!([2])),
            ("/data/real", json!(["2"])),
            ("/data/imag", json!([])),
            ("/data/rows/0", json!(0)),
            ("/data/rows/1", json!(4)),
            ("/data/columns/1", json!(5)),
            ("/data/rows", json!([1, 1])),
            ("/class", json!("single")),
            ("/class", json!("logical")),
            ("/source_storage", json!("dense")),
        ] {
            let mut changed = serde_json::to_value(&wire).unwrap();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            // Changing rows alone to [1,1] is ordered while columns differ;
            // make that selected case an actual duplicate coordinate pair.
            if pointer == "/data/rows" && changed["data"]["rows"] == json!([1, 1]) {
                changed["data"]["columns"] = json!([2, 2]);
            }
            assert!(
                decode_value(&changed, &request).is_err(),
                "{pointer}: {changed}"
            );
        }
        let mut changed = serde_json::to_value(&wire).unwrap();
        changed["data"]["columns"] = json!([4, 2]);
        assert!(decode_value(&changed, &request).is_err());
        let mut same_column = serde_json::to_value(&wire).unwrap();
        same_column["data"]["columns"] = json!([2, 2]);
        assert!(decode_value(&same_column, &request).is_ok());
    }

    #[test]
    fn empty_nd_pages_keep_full_shape_and_empty_higher_axes_are_explicitly_unsupported() {
        let empty_request = request(30, 12, &[2]);
        let mut wire = dense(&empty_request, &[0, 3, 2], NumericClass::Double, &[]);
        wire.selection.rows = 0;
        wire.selection.columns = 3;
        let page = wire.decode(&empty_request).unwrap();
        assert_eq!(page.source_shape, decimals(&[0, 3, 2]));
        assert_eq!((page.selection.rows, page.selection.columns), (0, 3));
        assert_eq!(real(page), NumericComponents::Double(vec![]));
        let request = request(2, 3, &[1]);
        let wire = dense(&request, &[2, 3, 0], NumericClass::Double, &[]);
        assert!(matches!(
            wire.decode(&request),
            Err(ValuePageError::Unsupported {
                field: "window.higher_indices",
                ..
            })
        ));
        assert_eq!(shape_product(&decimals(&[1 << 40, 1 << 40, 0])).unwrap(), 0);
        assert!(shape_product(&decimals(&[1 << 40, 1 << 40])).is_err());
    }

    #[test]
    fn root_identity_structured_path_and_schema_are_checked_as_a_whole() {
        let mut request = request(1, 1, &[]);
        request.path.steps = vec![
            ValuePathStep::Cell {
                indices: decimals(&[1, 2]),
            },
            ValuePathStep::Field {
                indices: decimals(&[1, 1]),
                name: "exact".into(),
            },
        ];
        let wire = dense(
            &request,
            &[1, 1],
            NumericClass::Uint64,
            &["9007199254741009"],
        );
        let value = serde_json::to_value(&wire).unwrap();
        assert_eq!(
            decode_value(&value, &request).unwrap().token,
            request.path.root
        );
        assert_eq!(
            serde_json::from_value::<ValuePageRequest>(serde_json::to_value(&request).unwrap())
                .unwrap(),
            request
        );
        for (pointer, replacement) in [
            ("/schema_version", json!(2)),
            ("/token/context/worker_id", json!("other-worker")),
            ("/token/context/generation", json!("3")),
            ("/token/context/invocation", json!("4")),
            ("/token/context/revision", json!("18446744073709551615")),
            ("/token/name", json!("B")),
            ("/path/1/name", json!("different")),
            ("/path/0/indices/0", json!("2")),
        ] {
            let mut changed = value.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(decode_value(&changed, &request).is_err(), "{pointer}");
        }
        let mut changed = value;
        changed["source_shape"] = json!(["9007199254740992", "1"]);
        assert!(matches!(
            decode_value(&changed, &request),
            Err(ValuePageError::Unsupported {
                field: "source_shape",
                ..
            })
        ));
    }

    #[test]
    fn json_byte_gate_precedes_parse_and_unknown_fields_or_untyped_numbers_do_not_slip_through() {
        let request = request(1, 1, &[]);
        assert!(matches!(
            decode_json(&vec![b'!'; MAX_PAYLOAD_BYTES + 1], &request),
            Err(ValuePageError::Limit {
                field: "typed-page payload bytes",
                ..
            })
        ));
        let wire = dense(&request, &[1, 1], NumericClass::Double, &["1"]);
        let encoded = serde_json::to_string(&wire).unwrap();
        let duplicate = encoded.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        );
        assert_ne!(duplicate, encoded);
        assert!(matches!(
            decode_json(duplicate.as_bytes(), &request),
            Err(ValuePageError::Json(_))
        ));
        let mut unknown = serde_json::to_value(&wire).unwrap();
        unknown["unrecognized"] = json!(true);
        assert!(matches!(
            decode_value(&unknown, &request),
            Err(ValuePageError::Json(_))
        ));
        let mut unknown = serde_json::to_value(&wire).unwrap();
        unknown["token"]["context"]["unrecognized"] = json!(true);
        assert!(matches!(
            decode_value(&unknown, &request),
            Err(ValuePageError::Json(_))
        ));
        for (pointer, replacement) in [
            ("/source_shape/0", json!(1)),
            ("/selection/rows", json!(1.5)),
            ("/data/real/0", json!(1)),
            ("/data/real/0", json!("١")),
            ("/data/kind", json!("raw_memory")),
        ] {
            let mut changed = serde_json::to_value(&wire).unwrap();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(decode_value(&changed, &request).is_err(), "{pointer}");
        }
        let mut oversized = wire;
        let WirePageData::Dense { real, .. } = &mut oversized.data else {
            unreachable!()
        };
        real[0] = "1".repeat(MAX_COMPONENT_BYTES + 1);
        assert!(matches!(
            oversized.decode(&request),
            Err(ValuePageError::Limit {
                field: "component token bytes",
                ..
            })
        ));
    }
}
