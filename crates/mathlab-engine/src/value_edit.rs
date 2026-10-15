// SPDX-License-Identifier: GPL-3.0-or-later
//! CP03 range-edit wire types, exact components and checked receipts.
//!
//! The execution backend owns atomic replacement and history in one interpreter
//! operation. This codec does not enable a worker capability or dispatch writes;
//! native assignment, COW and interruption behavior require runtime qualification.

use crate::{
    BindingToken, DecimalU64, NumericClass, NumericComponents, PageData, PageSelection, PageWindow,
    SessionContext, SourceStorage, ValuePageError, ValuePageRequest, ValuePath, ValuePathStep,
    WirePageData, WireValuePage,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

pub const CAP_ATOMIC_RANGE_EDIT: &str = "atomic_range_edit.v1";
pub const EDIT_SCHEMA_VERSION: u32 = 1;
pub const MAX_EDIT_REQUEST_BYTES: usize = 1024 * 1024;
pub const MAX_EDIT_RECEIPT_BYTES: usize = 16 * 1024;
pub const MAX_HISTORY_REQUEST_BYTES: usize = 16 * 1024;
pub const MAX_PATCH_CELLS: usize = 2_000;
pub const MAX_RECORD_BYTES: usize = 512 * 1024;
pub const MAX_HISTORY_RECORDS: usize = 32;
pub const MAX_HISTORY_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_INFLIGHT_PATCH_BYTES: usize = 1024 * 1024;
const MAX_BACKEND_INDEX: u64 = (1_u64 << 53) - 1;

pub type EditResult<T> = Result<T, EditError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    Json(String),
    Invalid(String),
    Unsupported(String),
    StaleBinding,
    HistoryConflict(String),
    Limit {
        resource: &'static str,
        actual: u64,
        maximum: u64,
    },
    Backend(String),
    /// A revision advanced, a commit may have occurred, or its receipt is not
    /// authoritative. Refresh/reset history; never automatically replay.
    UncertainCommit(String),
    InvalidReceipt(String),
    Exhausted,
}

impl fmt::Display for EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(message) => write!(formatter, "Invalid edit JSON: {message}"),
            Self::Invalid(message) => write!(formatter, "Invalid range edit: {message}"),
            Self::Unsupported(message) => write!(formatter, "Unsupported range edit: {message}"),
            Self::StaleBinding => {
                formatter.write_str("The workspace binding changed; refresh before editing.")
            }
            Self::HistoryConflict(message) => write!(formatter, "History conflict: {message}"),
            Self::Limit {
                resource,
                actual,
                maximum,
            } => write!(
                formatter,
                "{resource} exceeds its limit ({actual} > {maximum})"
            ),
            Self::Backend(message) => write!(formatter, "Range preparation failed: {message}"),
            Self::UncertainCommit(message) => write!(
                formatter,
                "Range commit outcome is uncertain: {message}; do not retry automatically"
            ),
            Self::InvalidReceipt(message) => write!(formatter, "Invalid range receipt: {message}"),
            Self::Exhausted => {
                formatter.write_str("History identity exhausted; restart the session.")
            }
        }
    }
}
impl std::error::Error for EditError {}
impl From<ValuePageError> for EditError {
    fn from(error: ValuePageError) -> Self {
        Self::Invalid(error.to_string())
    }
}

pub(crate) fn bounded(resource: &'static str, actual: usize, maximum: usize) -> EditResult<()> {
    if actual > maximum {
        Err(EditError::Limit {
            resource,
            actual: actual as u64,
            maximum: maximum as u64,
        })
    } else {
        Ok(())
    }
}

fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        })
}

fn preflight_token(token: &BindingToken) -> EditResult<()> {
    if !identifier(&token.name)
        || token.context.worker_id.is_empty()
        || token.context.worker_id.len() > 256
    {
        return Err(EditError::Invalid(
            "invalid root identifier or worker identity".into(),
        ));
    }
    Ok(())
}

/// Public DTOs can be constructed without decoding JSON. Bound every borrowed
/// collection/string BEFORE making the bounded copies used by the shared codec.
fn preflight(path: &ValuePath, window: &PageWindow, descriptor: &LeafDescriptor) -> EditResult<()> {
    preflight_token(&path.root)?;
    bounded("path depth", path.steps.len(), 16)?;
    bounded("source rank", descriptor.source_shape.len(), 32)?;
    bounded("higher selectors", window.higher_indices.len(), 30)?;
    for step in &path.steps {
        let indices = match step {
            ValuePathStep::Cell { indices } => indices,
            ValuePathStep::Field { indices, name } => {
                if !identifier(name) {
                    return Err(EditError::Invalid("invalid field identifier".into()));
                }
                indices
            }
        };
        bounded("container subscript rank", indices.len(), 32)?;
    }
    Ok(())
}

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
        name: token.name,
        context: SessionContext {
            worker_id: token.context.worker_id,
            generation: token.context.generation.get(),
            invocation: token.context.invocation.get(),
            revision: token.context.revision.get(),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafDescriptor {
    pub class: NumericClass,
    pub source_shape: Vec<DecimalU64>,
    pub source_storage: SourceStorage,
    pub complex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentEncoding {
    Decimal,
    #[serde(rename = "ieee754_hex")]
    Ieee754Hex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditData {
    Dense {
        encoding: ComponentEncoding,
        real: Vec<String>,
        imag: Vec<String>,
    },
}

impl EditData {
    /// Encode already-typed Rust values. Floats travel as exact IEEE bits;
    /// integer values never pass through f64. The native backend must preserve
    /// accepted bits (including signed zero) or reject the operation. NaN
    /// payload preservation across later numerical operations is not promised.
    pub fn from_components(
        real: &NumericComponents,
        imag: Option<&NumericComponents>,
    ) -> EditResult<Self> {
        bounded("patch cells", real.len(), MAX_PATCH_CELLS)?;
        if real.is_empty()
            || imag.is_some_and(|values| {
                values.len() != real.len()
                    || component_class(values) != component_class(real)
                    || !matches!(
                        real,
                        NumericComponents::Single(_) | NumericComponents::Double(_)
                    )
            })
        {
            return Err(EditError::Invalid(
                "invalid typed component lengths/classes".into(),
            ));
        }
        // At most 4,000 component strings; every native integer needs <=20
        // bytes, floats <=16, logical one. Bound admission before formatting.
        bounded(
            "encoded component estimate",
            (real.len() + imag.map_or(0, NumericComponents::len)) * 24,
            MAX_EDIT_REQUEST_BYTES,
        )?;
        fn encode(values: &NumericComponents) -> Vec<String> {
            macro_rules! decimal {
                ($values:expr) => {
                    $values.iter().map(ToString::to_string).collect()
                };
            }
            match values {
                NumericComponents::Single(values) => values
                    .iter()
                    .map(|value| format!("{:08x}", value.to_bits()))
                    .collect(),
                NumericComponents::Double(values) => values
                    .iter()
                    .map(|value| format!("{:016x}", value.to_bits()))
                    .collect(),
                NumericComponents::Int8(values) => decimal!(values),
                NumericComponents::Uint8(values) => decimal!(values),
                NumericComponents::Int16(values) => decimal!(values),
                NumericComponents::Uint16(values) => decimal!(values),
                NumericComponents::Int32(values) => decimal!(values),
                NumericComponents::Uint32(values) => decimal!(values),
                NumericComponents::Int64(values) => decimal!(values),
                NumericComponents::Uint64(values) => decimal!(values),
                NumericComponents::Logical(values) => values
                    .iter()
                    .map(|value| if *value { "1" } else { "0" }.to_owned())
                    .collect(),
            }
        }
        Ok(Self::Dense {
            encoding: if matches!(
                real,
                NumericComponents::Single(_) | NumericComponents::Double(_)
            ) {
                ComponentEncoding::Ieee754Hex
            } else {
                ComponentEncoding::Decimal
            },
            real: encode(real),
            imag: imag.map_or_else(Vec::new, encode),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeEditRequest {
    pub schema_version: u32,
    pub path: ValuePath,
    pub window: PageWindow,
    pub expected_leaf: LeafDescriptor,
    pub data: EditData,
}

/// Every source axis is explicit. V1 selects a rectangle in axes 1/2 and fixes
/// all higher axes to extent one; this does NOT advertise thick N-D boxes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FullAxisRange {
    pub starts: Vec<DecimalU64>,
    pub extents: Vec<u32>,
}
impl FullAxisRange {
    pub fn cells(&self) -> EditResult<usize> {
        self.extents.iter().try_fold(1_usize, |product, extent| {
            product
                .checked_mul(*extent as usize)
                .ok_or_else(|| EditError::Invalid("range element count overflow".into()))
        })
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedRangeEdit {
    pub path: ValuePath,
    pub window: PageWindow,
    pub descriptor: LeafDescriptor,
    pub range: FullAxisRange,
    pub replacement: PageData,
}

impl RangeEditRequest {
    pub fn from_components(
        path: ValuePath,
        window: PageWindow,
        expected_leaf: LeafDescriptor,
        real: &NumericComponents,
        imag: Option<&NumericComponents>,
    ) -> EditResult<Self> {
        preflight(&path, &window, &expected_leaf)?;
        if component_class(real) != expected_leaf.class || imag.is_some() != expected_leaf.complex {
            return Err(EditError::Invalid(
                "typed components do not match the expected leaf".into(),
            ));
        }
        let request = Self {
            schema_version: EDIT_SCHEMA_VERSION,
            path,
            window,
            expected_leaf,
            data: EditData::from_components(real, imag)?,
        };
        request.validate_decode()?;
        Ok(request)
    }

    pub fn validate_decode(&self) -> EditResult<ValidatedRangeEdit> {
        if self.schema_version != EDIT_SCHEMA_VERSION {
            return Err(EditError::Invalid("expected schema version 1".into()));
        }
        preflight(&self.path, &self.window, &self.expected_leaf)?;
        ValuePageRequest {
            path: self.path.clone(),
            window: self.window.clone(),
        }
        .validate()?;
        let shape = &self.expected_leaf.source_shape;
        if !(2..=32).contains(&shape.len()) || self.window.higher_indices.len() != shape.len() - 2 {
            return Err(EditError::Invalid(
                "provide the full leaf rank and exactly one selector per higher axis".into(),
            ));
        }
        if shape.iter().any(|extent| extent.get() > MAX_BACKEND_INDEX) {
            return Err(EditError::Unsupported(
                "source extents must be below 2^53 in the initial backend".into(),
            ));
        }
        // Check zero before multiplication, even though a positive write into
        // an empty axis will subsequently reject rather than grow the value.
        if !shape.iter().any(|extent| extent.get() == 0) {
            shape
                .iter()
                .try_fold(1_u64, |product, extent| product.checked_mul(extent.get()))
                .ok_or_else(|| EditError::Invalid("source element count overflow".into()))?;
        }
        let mut starts = vec![self.window.row_start, self.window.column_start];
        starts.extend(self.window.higher_indices.iter().copied());
        let mut extents = vec![self.window.rows, self.window.columns];
        extents.resize(shape.len(), 1);
        for ((start, extent), source_extent) in starts.iter().zip(&extents).zip(shape) {
            if *extent == 0 || start.get() == 0 {
                return Err(EditError::Invalid(
                    "write starts/extents must be positive".into(),
                ));
            }
            let end = start
                .get()
                .checked_add(u64::from(*extent) - 1)
                .ok_or_else(|| EditError::Invalid("range end overflow".into()))?;
            if end > source_extent.get() {
                return Err(EditError::Invalid(
                    "the complete write must fit; clipping and resizing are forbidden".into(),
                ));
            }
        }
        if self.expected_leaf.complex
            && !matches!(
                self.expected_leaf.class,
                NumericClass::Single | NumericClass::Double
            )
        {
            return Err(EditError::Unsupported(
                "only single/double may be complex".into(),
            ));
        }
        if self.expected_leaf.source_storage == SourceStorage::Sparse
            && (shape.len() != 2
                || !matches!(
                    self.expected_leaf.class,
                    NumericClass::Double | NumericClass::Logical
                ))
        {
            return Err(EditError::Unsupported(
                "sparse edits support rank-two double/logical leaves".into(),
            ));
        }
        let range = FullAxisRange { starts, extents };
        let cells = range.cells()?;
        bounded("patch cells", cells, MAX_PATCH_CELLS)?;
        let EditData::Dense {
            encoding,
            real,
            imag,
        } = &self.data;
        if real.len() != cells
            || (self.expected_leaf.complex && imag.len() != cells)
            || (!self.expected_leaf.complex && !imag.is_empty())
            || real.iter().chain(imag).any(|text| text.len() > 32)
        {
            return Err(EditError::Invalid(
                "component counts or token lengths differ".into(),
            ));
        }
        let floats = matches!(
            self.expected_leaf.class,
            NumericClass::Single | NumericClass::Double
        );
        if (floats && *encoding != ComponentEncoding::Ieee754Hex)
            || (!floats && *encoding != ComponentEncoding::Decimal)
        {
            return Err(EditError::Invalid(
                "component encoding must match the numeric class".into(),
            ));
        }
        let dense = if floats {
            fn decode(class: NumericClass, tokens: &[String]) -> EditResult<NumericComponents> {
                let width = if class == NumericClass::Single { 8 } else { 16 };
                if tokens.iter().any(|text| {
                    text.len() != width
                        || !text
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }) {
                    return Err(EditError::Invalid(
                        "float components require fixed-width lowercase IEEE hexadecimal".into(),
                    ));
                }
                if class == NumericClass::Single {
                    tokens
                        .iter()
                        .map(|text| {
                            u32::from_str_radix(text, 16)
                                .map(f32::from_bits)
                                .map_err(|error| EditError::Invalid(error.to_string()))
                        })
                        .collect::<EditResult<Vec<_>>>()
                        .map(NumericComponents::Single)
                } else {
                    tokens
                        .iter()
                        .map(|text| {
                            u64::from_str_radix(text, 16)
                                .map(f64::from_bits)
                                .map_err(|error| EditError::Invalid(error.to_string()))
                        })
                        .collect::<EditResult<Vec<_>>>()
                        .map(NumericComponents::Double)
                }
            }
            PageData::Dense {
                real: decode(self.expected_leaf.class, real)?,
                imag: if self.expected_leaf.complex {
                    Some(decode(self.expected_leaf.class, imag)?)
                } else {
                    None
                },
            }
        } else {
            // Reuse the strict CP03 integer/logical codec; no float token enters
            // this path. This bounded Nx1 adapter is not a workspace inspection.
            let codec_request = ValuePageRequest {
                path: self.path.clone(),
                window: PageWindow {
                    row_start: 1.into(),
                    column_start: 1.into(),
                    rows: cells as u32,
                    columns: 1,
                    higher_indices: vec![],
                },
            };
            let wire = WireValuePage {
                schema_version: 1,
                token: self.path.root.clone(),
                path: self.path.steps.clone(),
                class: self.expected_leaf.class,
                source_shape: vec![(cells as u64).into(), 1.into()],
                source_storage: SourceStorage::Dense,
                complex: self.expected_leaf.complex,
                selection: PageSelection {
                    row_start: 1.into(),
                    column_start: 1.into(),
                    rows: cells as u32,
                    columns: 1,
                    higher_indices: vec![],
                },
                data: WirePageData::Dense {
                    real: real.clone(),
                    imag: imag.clone(),
                },
            };
            wire.decode(&codec_request)?.data
        };
        let replacement = canonical_for_storage(dense, &self.expected_leaf, &range)?;
        Ok(ValidatedRangeEdit {
            path: self.path.clone(),
            window: self.window.clone(),
            descriptor: self.expected_leaf.clone(),
            range,
            replacement,
        })
    }
}

pub fn decode_edit_json(bytes: &[u8]) -> EditResult<RangeEditRequest> {
    bounded("edit request bytes", bytes.len(), MAX_EDIT_REQUEST_BYTES)?;
    let request: RangeEditRequest =
        serde_json::from_slice(bytes).map_err(|error| EditError::Json(error.to_string()))?;
    request.validate_decode()?;
    Ok(request)
}

fn component_class(values: &NumericComponents) -> NumericClass {
    match values {
        NumericComponents::Double(_) => NumericClass::Double,
        NumericComponents::Single(_) => NumericClass::Single,
        NumericComponents::Int8(_) => NumericClass::Int8,
        NumericComponents::Uint8(_) => NumericClass::Uint8,
        NumericComponents::Int16(_) => NumericClass::Int16,
        NumericComponents::Uint16(_) => NumericClass::Uint16,
        NumericComponents::Int32(_) => NumericClass::Int32,
        NumericComponents::Uint32(_) => NumericClass::Uint32,
        NumericComponents::Int64(_) => NumericClass::Int64,
        NumericComponents::Uint64(_) => NumericClass::Uint64,
        NumericComponents::Logical(_) => NumericClass::Logical,
    }
}
fn zero(values: &NumericComponents, index: usize) -> bool {
    match values {
        NumericComponents::Double(v) => v[index] == 0.,
        NumericComponents::Single(v) => v[index] == 0.,
        NumericComponents::Int8(v) => v[index] == 0,
        NumericComponents::Uint8(v) => v[index] == 0,
        NumericComponents::Int16(v) => v[index] == 0,
        NumericComponents::Uint16(v) => v[index] == 0,
        NumericComponents::Int32(v) => v[index] == 0,
        NumericComponents::Uint32(v) => v[index] == 0,
        NumericComponents::Int64(v) => v[index] == 0,
        NumericComponents::Uint64(v) => v[index] == 0,
        NumericComponents::Logical(v) => !v[index],
    }
}
fn retain(values: &NumericComponents, indices: &[usize]) -> NumericComponents {
    macro_rules! select {
        ($values:expr, $variant:ident) => {
            NumericComponents::$variant(indices.iter().map(|index| $values[*index]).collect())
        };
    }
    match values {
        NumericComponents::Double(v) => select!(v, Double),
        NumericComponents::Single(v) => select!(v, Single),
        NumericComponents::Int8(v) => select!(v, Int8),
        NumericComponents::Uint8(v) => select!(v, Uint8),
        NumericComponents::Int16(v) => select!(v, Int16),
        NumericComponents::Uint16(v) => select!(v, Uint16),
        NumericComponents::Int32(v) => select!(v, Int32),
        NumericComponents::Uint32(v) => select!(v, Uint32),
        NumericComponents::Int64(v) => select!(v, Int64),
        NumericComponents::Uint64(v) => select!(v, Uint64),
        NumericComponents::Logical(v) => select!(v, Logical),
    }
}

fn validate_components(
    real: &NumericComponents,
    imag: &Option<NumericComponents>,
    descriptor: &LeafDescriptor,
    count: usize,
) -> EditResult<()> {
    if component_class(real) != descriptor.class
        || real.len() != count
        || imag.is_some() != descriptor.complex
        || imag.as_ref().is_some_and(|values| {
            component_class(values) != descriptor.class || values.len() != count
        })
    {
        return Err(EditError::Invalid(
            "patch class, complex state or component count changed".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_patch(
    data: &PageData,
    descriptor: &LeafDescriptor,
    range: &FullAxisRange,
) -> EditResult<()> {
    let cells = range.cells()?;
    bounded("captured patch cells", cells, MAX_PATCH_CELLS)?;
    match data {
        PageData::Dense { real, imag } if descriptor.source_storage == SourceStorage::Dense => {
            validate_components(real, imag, descriptor, cells)
        }
        PageData::SparseCoo {
            rows,
            columns,
            real,
            imag,
        } if descriptor.source_storage == SourceStorage::Sparse => {
            if range.extents.len() != 2 || rows.len() != columns.len() || rows.len() > cells {
                return Err(EditError::Invalid(
                    "invalid sparse patch extents/counts".into(),
                ));
            }
            validate_components(real, imag, descriptor, rows.len())?;
            let mut previous = None;
            for (offset, (&row, &column)) in rows.iter().zip(columns).enumerate() {
                if row == 0
                    || column == 0
                    || row > range.extents[0]
                    || column > range.extents[1]
                    || previous.is_some_and(|last| (column, row) <= last)
                    || (zero(real, offset)
                        && imag.as_ref().is_none_or(|values| zero(values, offset)))
                {
                    return Err(EditError::Invalid(
                        "sparse patch must be unique ordered local nonzero COO".into(),
                    ));
                }
                previous = Some((column, row));
            }
            Ok(())
        }
        _ => Err(EditError::Invalid(
            "patch storage must match the leaf; no densification".into(),
        )),
    }
}

fn canonical_for_storage(
    data: PageData,
    descriptor: &LeafDescriptor,
    range: &FullAxisRange,
) -> EditResult<PageData> {
    let PageData::Dense { real, imag } = data else {
        return Err(EditError::Invalid(
            "RHS must be dense typed components".into(),
        ));
    };
    validate_components(&real, &imag, descriptor, range.cells()?)?;
    let data = if descriptor.source_storage == SourceStorage::Dense {
        PageData::Dense { real, imag }
    } else {
        let selected = (0..real.len())
            .filter(|index| {
                !zero(&real, *index) || imag.as_ref().is_some_and(|values| !zero(values, *index))
            })
            .collect::<Vec<_>>();
        let rows = selected
            .iter()
            .map(|index| (*index % range.extents[0] as usize + 1) as u32)
            .collect();
        let columns = selected
            .iter()
            .map(|index| (*index / range.extents[0] as usize + 1) as u32)
            .collect();
        PageData::SparseCoo {
            rows,
            columns,
            real: retain(&real, &selected),
            imag: imag.as_ref().map(|values| retain(values, &selected)),
        }
    };
    validate_patch(&data, descriptor, range)?;
    Ok(data)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditOperation {
    Write,
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryHead {
    pub transaction_id: DecimalU64,
    pub root_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryHeads {
    pub epoch: DecimalU64,
    pub undo: Option<HistoryHead>,
    pub redo: Option<HistoryHead>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRequest {
    pub schema_version: u32,
    #[serde(deserialize_with = "strict_token")]
    pub expected: BindingToken,
    pub history_epoch: DecimalU64,
    pub transaction_id: DecimalU64,
}
impl HistoryRequest {
    pub fn validate(&self) -> EditResult<()> {
        preflight_token(&self.expected)?;
        if self.schema_version != EDIT_SCHEMA_VERSION
            || self.history_epoch.get() == 0
            || self.transaction_id.get() == 0
        {
            return Err(EditError::Invalid(
                "history schema, epoch and transaction must be valid".into(),
            ));
        }
        Ok(())
    }
}
pub fn decode_history_json(bytes: &[u8]) -> EditResult<HistoryRequest> {
    bounded(
        "history request bytes",
        bytes.len(),
        MAX_HISTORY_REQUEST_BYTES,
    )?;
    let request: HistoryRequest =
        serde_json::from_slice(bytes).map_err(|error| EditError::Json(error.to_string()))?;
    request.validate()?;
    Ok(request)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeEditReceipt {
    pub schema_version: u32,
    pub operation: EditOperation,
    pub transaction_id: DecimalU64,
    #[serde(deserialize_with = "strict_token")]
    pub token: BindingToken,
    pub history: HistoryHeads,
    /// Conservative logical encoded-patch charge, NOT allocator/RSS bytes.
    pub retained_patch_bytes: usize,
    pub retained_transactions: u32,
    pub evicted_undo_entries: u32,
}
pub struct ReceiptExpectation {
    pub operation: EditOperation,
    pub previous: BindingToken,
    /// Writes may assign a new owner ID; history operations require the exact ID.
    pub transaction_id: Option<DecimalU64>,
    pub history_epoch: Option<DecimalU64>,
}

pub(crate) fn next_token(previous: &BindingToken) -> EditResult<BindingToken> {
    preflight_token(previous)?;
    let mut token = previous.clone();
    token.context.revision = token
        .context
        .revision
        .checked_add(1)
        .ok_or(EditError::Exhausted)?;
    Ok(token)
}
impl RangeEditReceipt {
    pub fn validate(
        &self,
        expectation: &ReceiptExpectation,
        settled: &SessionContext,
    ) -> EditResult<()> {
        // Typed callers bypass JSON byte gates. Check all externally owned
        // strings before comparisons, identity clones or JSON serialization.
        preflight_token(&self.token)?;
        preflight_token(&expectation.previous)?;
        if settled.worker_id.is_empty() || settled.worker_id.len() > 256 {
            return Err(EditError::InvalidReceipt(
                "invalid settled worker identity".into(),
            ));
        }
        for head in [&self.history.undo, &self.history.redo]
            .into_iter()
            .flatten()
        {
            if head.transaction_id.get() == 0 || !identifier(&head.root_name) {
                return Err(EditError::InvalidReceipt(
                    "invalid history head identity".into(),
                ));
            }
        }
        let next = next_token(&expectation.previous)?;
        if self.schema_version != 1
            || self.operation != expectation.operation
            || self.token != next
            || &self.token.context != settled
            || self.transaction_id.get() == 0
            || self.history.epoch.get() == 0
            || expectation
                .transaction_id
                .is_some_and(|id| id != self.transaction_id)
            || expectation
                .history_epoch
                .is_some_and(|epoch| epoch != self.history.epoch)
            || (expectation.operation != EditOperation::Write
                && (expectation.transaction_id.is_none() || expectation.history_epoch.is_none()))
        {
            return Err(EditError::InvalidReceipt(
                "schema, action, transaction, epoch or exact new token/context mismatch".into(),
            ));
        }
        bounded(
            "retained transactions",
            self.retained_transactions as usize,
            MAX_HISTORY_RECORDS,
        )?;
        bounded(
            "retained logical patch bytes",
            self.retained_patch_bytes,
            MAX_HISTORY_BYTES,
        )?;
        bounded(
            "evicted undo count",
            self.evicted_undo_entries as usize,
            MAX_HISTORY_RECORDS,
        )?;
        let linked = HistoryHead {
            transaction_id: self.transaction_id,
            root_name: self.token.name.clone(),
        };
        let head = if self.operation == EditOperation::Undo {
            &self.history.redo
        } else {
            &self.history.undo
        };
        if head.as_ref() != Some(&linked)
            || (self.operation == EditOperation::Write && self.history.redo.is_some())
            || (self.operation != EditOperation::Write && self.evicted_undo_entries != 0)
            || self.retained_transactions == 0
            || self.retained_patch_bytes == 0
            || self
                .history
                .undo
                .as_ref()
                .zip(self.history.redo.as_ref())
                .is_some_and(|(undo, redo)| undo.transaction_id == redo.transaction_id)
        {
            return Err(EditError::InvalidReceipt(
                "history heads, bounds or retained counts are inconsistent".into(),
            ));
        }
        let heads =
            usize::from(self.history.undo.is_some()) + usize::from(self.history.redo.is_some());
        if heads > self.retained_transactions as usize {
            return Err(EditError::InvalidReceipt(
                "more heads than retained records".into(),
            ));
        }
        bounded(
            "edit receipt bytes",
            serde_json::to_vec(self)
                .map_err(|error| EditError::Json(error.to_string()))?
                .len(),
            MAX_EDIT_RECEIPT_BYTES,
        )
    }
}
pub fn decode_receipt_json(
    bytes: &[u8],
    expectation: &ReceiptExpectation,
    settled: &SessionContext,
) -> EditResult<RangeEditReceipt> {
    bounded("edit receipt bytes", bytes.len(), MAX_EDIT_RECEIPT_BYTES)?;
    let receipt: RangeEditReceipt =
        serde_json::from_slice(bytes).map_err(|error| EditError::Json(error.to_string()))?;
    receipt.validate(expectation, settled)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token() -> BindingToken {
        BindingToken {
            name: "A".into(),
            context: SessionContext {
                worker_id: "wire-test-worker".into(),
                generation: 3,
                invocation: 2,
                revision: 17,
            },
        }
    }
    fn request(class: NumericClass, values: NumericComponents) -> RangeEditRequest {
        RangeEditRequest::from_components(
            ValuePath {
                root: token(),
                steps: vec![],
            },
            PageWindow {
                row_start: 1.into(),
                column_start: 1.into(),
                rows: values.len() as u32,
                columns: 1,
                higher_indices: vec![],
            },
            LeafDescriptor {
                class,
                source_shape: vec![(values.len() as u64).into(), 1.into()],
                source_storage: SourceStorage::Dense,
                complex: false,
            },
            &values,
            None,
        )
        .unwrap()
    }
    fn receipt(operation: EditOperation) -> (RangeEditReceipt, ReceiptExpectation) {
        let old = token();
        let next = next_token(&old).unwrap();
        let linked = HistoryHead {
            transaction_id: 7.into(),
            root_name: "A".into(),
        };
        let heads = if operation == EditOperation::Undo {
            HistoryHeads {
                epoch: 4.into(),
                undo: None,
                redo: Some(linked),
            }
        } else {
            HistoryHeads {
                epoch: 4.into(),
                undo: Some(linked),
                redo: None,
            }
        };
        (
            RangeEditReceipt {
                schema_version: 1,
                operation,
                transaction_id: 7.into(),
                token: next,
                history: heads,
                retained_patch_bytes: 1024,
                retained_transactions: 1,
                evicted_undo_entries: 0,
            },
            ReceiptExpectation {
                operation,
                previous: old,
                transaction_id: Some(7.into()),
                history_epoch: Some(4.into()),
            },
        )
    }

    #[test]
    fn integer_extremes_remain_exact_decimal_and_native_types() {
        let unsigned = request(
            NumericClass::Uint64,
            NumericComponents::Uint64(vec![9_007_199_254_741_009, u64::MAX]),
        );
        let EditData::Dense { encoding, real, .. } = &unsigned.data;
        assert_eq!(*encoding, ComponentEncoding::Decimal);
        assert_eq!(real, &["9007199254741009", "18446744073709551615"]);
        let decoded = unsigned.validate_decode().unwrap();
        assert!(
            matches!(decoded.replacement, PageData::Dense { real: NumericComponents::Uint64(values), imag: None } if values == [9_007_199_254_741_009, u64::MAX])
        );
        let signed = request(
            NumericClass::Int64,
            NumericComponents::Int64(vec![i64::MIN, i64::MAX]),
        );
        let bytes = serde_json::to_vec(&signed).unwrap();
        assert_eq!(decode_edit_json(&bytes).unwrap(), signed);
    }

    #[test]
    fn ieee_bits_preserve_single_double_signed_zero_and_special_values() {
        let bits32 = [
            0x3f80_0001,
            0x8000_0000,
            0x0000_0001,
            0x7f80_0000,
            0x7fc0_0042,
        ];
        let input32 = NumericComponents::Single(bits32.into_iter().map(f32::from_bits).collect());
        let edit = request(NumericClass::Single, input32);
        let EditData::Dense { encoding, real, .. } = &edit.data;
        assert_eq!(*encoding, ComponentEncoding::Ieee754Hex);
        assert_eq!(
            real,
            &["3f800001", "80000000", "00000001", "7f800000", "7fc00042"]
        );
        let PageData::Dense {
            real: NumericComponents::Single(values),
            ..
        } = edit.validate_decode().unwrap().replacement
        else {
            panic!("single components expected")
        };
        assert_eq!(
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            bits32
        );
        let bits64 = [
            0x8000_0000_0000_0000,
            1,
            0xfff0_0000_0000_0000,
            0x7ff8_0000_0000_0042,
        ];
        let edit = request(
            NumericClass::Double,
            NumericComponents::Double(bits64.into_iter().map(f64::from_bits).collect()),
        );
        let PageData::Dense {
            real: NumericComponents::Double(values),
            ..
        } = edit.validate_decode().unwrap().replacement
        else {
            panic!("double components expected")
        };
        assert_eq!(
            values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            bits64
        );
    }

    #[test]
    fn wrong_float_encoding_width_case_and_decimal_tokens_reject() {
        let base = request(NumericClass::Single, NumericComponents::Single(vec![1.]));
        for text in [
            "3F800000",
            "3f80000",
            "03f800000",
            "1.0",
            "0000000000000000",
            "0x3f800000",
        ] {
            let mut bad = base.clone();
            let EditData::Dense { real, .. } = &mut bad.data;
            real[0] = text.into();
            assert!(bad.validate_decode().is_err(), "accepted {text}");
        }
        let mut bad = base;
        let EditData::Dense { encoding, .. } = &mut bad.data;
        *encoding = ComponentEncoding::Decimal;
        assert!(bad.validate_decode().is_err());
        let mut integer = request(NumericClass::Uint64, NumericComponents::Uint64(vec![1]));
        let EditData::Dense { encoding, .. } = &mut integer.data;
        *encoding = ComponentEncoding::Ieee754Hex;
        assert!(integer.validate_decode().is_err());
    }

    #[test]
    fn decimal_integer_tokens_are_canonical_and_never_json_numbers() {
        let base = request(NumericClass::Uint64, NumericComponents::Uint64(vec![1]));
        for text in ["+1", "01", "1.0", "1e0", "-1", "18446744073709551616"] {
            let mut bad = base.clone();
            let EditData::Dense { real, .. } = &mut bad.data;
            real[0] = text.into();
            assert!(bad.validate_decode().is_err(), "accepted {text}");
        }
        let mut numeric = serde_json::to_value(&base).unwrap();
        numeric["data"]["real"][0] = json!(1);
        assert!(decode_edit_json(&serde_json::to_vec(&numeric).unwrap()).is_err());
        numeric = serde_json::to_value(base).unwrap();
        numeric["path"]["root"]["context"]["revision"] = json!(17);
        assert!(decode_edit_json(&serde_json::to_vec(&numeric).unwrap()).is_err());
    }

    #[test]
    fn exact_full_axis_plane_selection_never_clips_or_grows() {
        let mut edit = request(
            NumericClass::Uint64,
            NumericComponents::Uint64(vec![101, 102, 103, 104]),
        );
        edit.expected_leaf.source_shape = vec![3.into(), 4.into(), 5.into()];
        edit.window = PageWindow {
            row_start: 2.into(),
            column_start: 3.into(),
            rows: 2,
            columns: 2,
            higher_indices: vec![4.into()],
        };
        let valid = edit.validate_decode().unwrap();
        assert_eq!(valid.range.starts, vec![2.into(), 3.into(), 4.into()]);
        assert_eq!(valid.range.extents, vec![2, 2, 1]);
        assert_eq!(valid.range.cells().unwrap(), 4);
        for (row, column, plane) in [(3, 3, 4), (2, 4, 4), (2, 3, 6)] {
            let mut bad = edit.clone();
            bad.window.row_start = row.into();
            bad.window.column_start = column.into();
            bad.window.higher_indices[0] = plane.into();
            assert!(bad.validate_decode().is_err());
        }
        edit.window.higher_indices.clear();
        assert!(edit.validate_decode().is_err());
        edit.expected_leaf.source_shape = vec![0.into(), 3.into()];
        assert!(
            edit.validate_decode().is_err(),
            "empty leaf cannot grow through an edit"
        );
    }

    #[test]
    fn sparse_rhs_canonicalization_keeps_only_local_nonzeros() {
        let mut edit = request(
            NumericClass::Double,
            NumericComponents::Double(vec![0., 20., 10., -0., 0., 30.]),
        );
        edit.expected_leaf.source_shape = vec![3.into(), 4.into()];
        edit.expected_leaf.source_storage = SourceStorage::Sparse;
        edit.window = PageWindow {
            row_start: 2.into(),
            column_start: 2.into(),
            rows: 2,
            columns: 3,
            higher_indices: vec![],
        };
        let PageData::SparseCoo {
            rows,
            columns,
            real: NumericComponents::Double(values),
            imag: None,
        } = edit.validate_decode().unwrap().replacement
        else {
            panic!("sparse patch expected")
        };
        assert_eq!(rows, [2, 1, 2]);
        assert_eq!(columns, [1, 2, 3]);
        assert_eq!(values, [20., 10., 30.]);
        // The worker must replace the whole selected rectangle: absent COO
        // entries clear its previous values, rather than merging nonzeros.
        edit.expected_leaf.class = NumericClass::Single;
        assert!(edit.validate_decode().is_err());
    }

    #[test]
    fn typed_constructor_rejects_class_complex_and_size_mismatches() {
        let base = request(NumericClass::Uint64, NumericComponents::Uint64(vec![1]));
        assert!(
            RangeEditRequest::from_components(
                base.path.clone(),
                base.window.clone(),
                base.expected_leaf.clone(),
                &NumericComponents::Int64(vec![1]),
                None
            )
            .is_err()
        );
        assert!(
            EditData::from_components(
                &NumericComponents::Uint64(vec![1]),
                Some(&NumericComponents::Uint64(vec![0]))
            )
            .is_err()
        );
        assert!(
            EditData::from_components(
                &NumericComponents::Single(vec![1.]),
                Some(&NumericComponents::Double(vec![0.]))
            )
            .is_err()
        );
        assert!(
            EditData::from_components(
                &NumericComponents::Uint8(vec![0; MAX_PATCH_CELLS + 1]),
                None
            )
            .is_err()
        );
        let mut complex = request(NumericClass::Double, NumericComponents::Double(vec![1.]));
        complex.expected_leaf.complex = true;
        assert!(complex.validate_decode().is_err());
        complex.data = EditData::from_components(
            &NumericComponents::Double(vec![1.]),
            Some(&NumericComponents::Double(vec![-0.])),
        )
        .unwrap();
        assert!(complex.validate_decode().is_ok());
    }

    #[test]
    fn json_limits_unknown_duplicate_fields_and_public_collection_bounds_reject() {
        assert!(matches!(
            decode_edit_json(&vec![b'!'; MAX_EDIT_REQUEST_BYTES + 1]),
            Err(EditError::Limit {
                resource: "edit request bytes",
                ..
            })
        ));
        let base = request(NumericClass::Uint64, NumericComponents::Uint64(vec![1]));
        let text = String::from_utf8(serde_json::to_vec(&base).unwrap()).unwrap();
        let duplicate = text.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        );
        assert!(decode_edit_json(duplicate.as_bytes()).is_err());
        let mut unknown = serde_json::to_value(&base).unwrap();
        unknown["expected_leaf"]["execute"] = json!("forbidden");
        assert!(decode_edit_json(&serde_json::to_vec(&unknown).unwrap()).is_err());
        unknown = serde_json::to_value(&base).unwrap();
        unknown["path"]["root"]["context"]["extra"] = json!(true);
        assert!(decode_edit_json(&serde_json::to_vec(&unknown).unwrap()).is_err());
        let mut bad = base.clone();
        bad.path.steps = vec![
            ValuePathStep::Cell {
                indices: vec![1.into(), 1.into()]
            };
            17
        ];
        assert!(matches!(
            bad.validate_decode(),
            Err(EditError::Limit {
                resource: "path depth",
                ..
            })
        ));
        bad = base.clone();
        bad.path.steps.push(ValuePathStep::Cell {
            indices: vec![1.into(); 33],
        });
        assert!(matches!(
            bad.validate_decode(),
            Err(EditError::Limit {
                resource: "container subscript rank",
                ..
            })
        ));
        bad = base;
        bad.expected_leaf.source_shape = vec![1.into(); 33];
        assert!(matches!(
            bad.validate_decode(),
            Err(EditError::Limit {
                resource: "source rank",
                ..
            })
        ));
    }

    #[test]
    fn receipt_requires_exact_new_context_action_head_and_limits() {
        for operation in [
            EditOperation::Write,
            EditOperation::Undo,
            EditOperation::Redo,
        ] {
            let (valid, expectation) = receipt(operation);
            let settled = valid.token.context.clone();
            let encoded = serde_json::to_vec(&valid).unwrap();
            assert_eq!(
                decode_receipt_json(&encoded, &expectation, &settled).unwrap(),
                valid
            );
            let mut bad = valid.clone();
            bad.token.context.worker_id = "different-worker".into();
            assert!(bad.validate(&expectation, &settled).is_err());
            bad = valid.clone();
            bad.token.context.revision -= 1;
            assert!(bad.validate(&expectation, &settled).is_err());
            bad = valid.clone();
            bad.transaction_id = 8.into();
            assert!(bad.validate(&expectation, &settled).is_err());
            bad = valid.clone();
            bad.history.epoch = 5.into();
            assert!(bad.validate(&expectation, &settled).is_err());
            bad = valid.clone();
            bad.retained_patch_bytes = MAX_HISTORY_BYTES + 1;
            assert!(bad.validate(&expectation, &settled).is_err());
            let mut pending = settled.clone();
            pending.invocation += 1;
            assert!(valid.validate(&expectation, &pending).is_err());
        }
    }

    #[test]
    fn receipt_rejects_unknown_duplicate_numeric_identity_and_oversized_envelope() {
        let (valid, expectation) = receipt(EditOperation::Undo);
        let settled = valid.token.context.clone();
        assert!(matches!(
            decode_receipt_json(
                &vec![b'!'; MAX_EDIT_RECEIPT_BYTES + 1],
                &expectation,
                &settled
            ),
            Err(EditError::Limit {
                resource: "edit receipt bytes",
                ..
            })
        ));
        let mut bad = serde_json::to_value(&valid).unwrap();
        bad["transaction_id"] = json!(7);
        assert!(
            decode_receipt_json(&serde_json::to_vec(&bad).unwrap(), &expectation, &settled)
                .is_err()
        );
        bad = serde_json::to_value(&valid).unwrap();
        bad["token"]["extra"] = json!(true);
        assert!(
            decode_receipt_json(&serde_json::to_vec(&bad).unwrap(), &expectation, &settled)
                .is_err()
        );
        let text = String::from_utf8(serde_json::to_vec(&valid).unwrap()).unwrap();
        let duplicate = text.replacen(
            "\"transaction_id\":\"7\"",
            "\"transaction_id\":\"7\",\"transaction_id\":\"7\"",
            1,
        );
        assert!(decode_receipt_json(duplicate.as_bytes(), &expectation, &settled).is_err());
    }

    #[test]
    fn history_request_and_typed_receipt_identities_are_bounded() {
        let request = HistoryRequest {
            schema_version: 1,
            expected: token(),
            history_epoch: 4.into(),
            transaction_id: 7.into(),
        };
        assert_eq!(
            decode_history_json(&serde_json::to_vec(&request).unwrap()).unwrap(),
            request
        );
        let mut bad = request.clone();
        bad.transaction_id = 0.into();
        assert!(bad.validate().is_err());
        bad = request;
        bad.expected.name = "A".repeat(64);
        assert!(bad.validate().is_err());
        assert!(matches!(
            decode_history_json(&vec![b'!'; MAX_HISTORY_REQUEST_BYTES + 1]),
            Err(EditError::Limit {
                resource: "history request bytes",
                ..
            })
        ));
        let (mut receipt, mut expectation) = receipt(EditOperation::Write);
        let settled = receipt.token.context.clone();
        receipt.token.context.worker_id = "w".repeat(257);
        assert!(receipt.validate(&expectation, &settled).is_err());
        receipt.token.context = settled.clone();
        expectation.previous.name = "A".repeat(64);
        assert!(receipt.validate(&expectation, &settled).is_err());
        expectation.previous.name = "A".into();
        receipt.history.undo.as_mut().unwrap().root_name = "A".repeat(64);
        assert!(receipt.validate(&expectation, &settled).is_err());
    }
}
