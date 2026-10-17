// SPDX-License-Identifier: GPL-3.0-or-later
//! Real-double construction, elementwise arithmetic and matrix multiplication.
//!
//! This module is unregistered preparation, not a MATLAB parser or backend.
//! Its resource policies and errors are project policies. Binary elementwise
//! arithmetic uses one primitive `f64` +, -, * or / per output element;
//! arithmetic NaN bits are not a compatibility guarantee.

#![forbid(unsafe_code)]

#[cfg(not(target_pointer_width = "64"))]
compile_error!("RN-PREP-01 v1 currently qualifies only 64-bit Rust targets");

const ELEMENT_WIDTH_BYTES: u64 = 8;

mod elementwise_products;
mod mtimes;
mod subtraction;
mod transpose;
mod unary;
pub use mtimes::{MtimesErrorV1, MtimesWorkBudgetV1};

/// Limits for one newly constructed or output logical payload.
///
/// These do not bound aggregate memory, allocator padding, or process RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitsV1 {
    pub max_dimension: u64,
    pub max_elements: u64,
    pub max_bytes: u64,
}

impl Default for LimitsV1 {
    fn default() -> Self {
        Self {
            max_dimension: 1_048_576,
            max_elements: 1_048_576,
            max_bytes: 8_388_608,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorContextV1 {
    Construct,
    AddCompatibility,
    AddOutput,
    MtimesOutput,
    SubCompatibility,
    SubOutput,
    TimesCompatibility,
    TimesOutput,
    RDivideCompatibility,
    RDivideOutput,
    TransposeOutput,
}

impl ErrorContextV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Construct => "construct",
            Self::AddCompatibility => "add.compatibility",
            Self::AddOutput => "add.output",
            Self::SubCompatibility => "sub.compatibility",
            Self::SubOutput => "sub.output",
            Self::TimesCompatibility => "times.compatibility",
            Self::TimesOutput => "times.output",
            Self::RDivideCompatibility => "rdivide.compatibility",
            Self::RDivideOutput => "rdivide.output",
            Self::TransposeOutput => "transpose.output",
            Self::MtimesOutput => "mtimes.output",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCodeV1 {
    ShapeProductOverflow {
        rows: u64,
        columns: u64,
    },
    ByteSizeOverflow {
        elements: u64,
        element_width_bytes: u64,
    },
    DimensionLimitExceeded {
        axis: usize,
        actual: u64,
        limit: u64,
    },
    ElementLimitExceeded {
        actual: u64,
        limit: u64,
    },
    ByteLimitExceeded {
        actual: u64,
        limit: u64,
    },
    AddressSpaceExceeded {
        elements: u64,
        payload_bytes: u64,
        pointer_width_bits: u32,
    },
    StorageLengthMismatch {
        expected_elements: u64,
        actual_elements: u64,
    },
    IncompatibleShape {
        axis: usize,
        lhs_extent: u64,
        rhs_extent: u64,
    },
    AllocationFailure {
        requested_elements: u64,
        requested_bytes: u64,
    },
}

impl ErrorCodeV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShapeProductOverflow { .. } => "ShapeProductOverflow",
            Self::ByteSizeOverflow { .. } => "ByteSizeOverflow",
            Self::DimensionLimitExceeded { .. } => "DimensionLimitExceeded",
            Self::ElementLimitExceeded { .. } => "ElementLimitExceeded",
            Self::ByteLimitExceeded { .. } => "ByteLimitExceeded",
            Self::AddressSpaceExceeded { .. } => "AddressSpaceExceeded",
            Self::StorageLengthMismatch { .. } => "StorageLengthMismatch",
            Self::IncompatibleShape { .. } => "IncompatibleShape",
            Self::AllocationFailure { .. } => "AllocationFailure",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayErrorV1 {
    pub context: ErrorContextV1,
    pub code: ErrorCodeV1,
}

/// An invariant-bearing, independently owned column-major 2-D real array.
///
/// Borrowed public references cannot mutate shape or storage. Consuming
/// transformations preserve layout invariants. There is no infallible array
/// clone or implicit fallback to a different runtime.
#[derive(Debug)]
pub struct DenseDouble2d {
    shape: [u64; 2],
    data: Vec<f64>,
}

#[derive(Debug, Clone, Copy)]
enum ElementwiseOperation {
    Add,
    Subtract,
    Times,
    RDivide,
}

impl ElementwiseOperation {
    fn compatibility_context(self) -> ErrorContextV1 {
        match self {
            Self::Add => ErrorContextV1::AddCompatibility,
            Self::Subtract => ErrorContextV1::SubCompatibility,
            Self::Times => ErrorContextV1::TimesCompatibility,
            Self::RDivide => ErrorContextV1::RDivideCompatibility,
        }
    }
    fn output_context(self) -> ErrorContextV1 {
        match self {
            Self::Add => ErrorContextV1::AddOutput,
            Self::Subtract => ErrorContextV1::SubOutput,
            Self::Times => ErrorContextV1::TimesOutput,
            Self::RDivide => ErrorContextV1::RDivideOutput,
        }
    }
    fn apply(self, lhs: f64, rhs: f64) -> f64 {
        match self {
            Self::Add => lhs + rhs,
            Self::Subtract => lhs - rhs,
            Self::Times => lhs * rhs,
            Self::RDivide => lhs / rhs,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CheckedLayout {
    elements: u64,
    payload_bytes: u64,
    len: usize,
}

impl DenseDouble2d {
    pub fn try_from_col_major(
        shape: [u64; 2],
        source: &[f64],
        limits: LimitsV1,
    ) -> Result<Self, ArrayErrorV1> {
        Self::from_col_major_with_reservation(shape, source, limits, reserve_payload)
    }

    pub fn try_add(&self, rhs: &Self, output_limits: LimitsV1) -> Result<Self, ArrayErrorV1> {
        self.add_with_reservation(rhs, output_limits, reserve_payload)
    }

    pub fn shape(&self) -> [u64; 2] {
        self.shape
    }

    pub fn as_slice(&self) -> &[f64] {
        &self.data
    }

    fn from_col_major_with_reservation<F>(
        shape: [u64; 2],
        source: &[f64],
        limits: LimitsV1,
        reserve: F,
    ) -> Result<Self, ArrayErrorV1>
    where
        F: FnOnce(&mut Vec<f64>, CheckedLayout, ErrorContextV1) -> Result<(), ArrayErrorV1>,
    {
        let context = ErrorContextV1::Construct;
        let layout = checked_layout(shape, limits, context)?;
        if source.len() != layout.len {
            return Err(ArrayErrorV1 {
                context,
                code: ErrorCodeV1::StorageLengthMismatch {
                    expected_elements: layout.elements,
                    // This module is explicitly restricted to 64-bit targets.
                    actual_elements: source.len() as u64,
                },
            });
        }
        let mut data = Vec::new();
        if layout.len == 0 {
            return Ok(Self { shape, data });
        }
        reserve(&mut data, layout, context)?;
        // Reservation covers the complete, already-validated source length.
        // Copying f64 values performs no arithmetic or string conversion.
        data.extend_from_slice(source);
        Ok(Self { shape, data })
    }

    fn add_with_reservation<F>(
        &self,
        rhs: &Self,
        output_limits: LimitsV1,
        reserve: F,
    ) -> Result<Self, ArrayErrorV1>
    where
        F: FnOnce(&mut Vec<f64>, CheckedLayout, ErrorContextV1) -> Result<(), ArrayErrorV1>,
    {
        self.elementwise_with_reservation(rhs, output_limits, ElementwiseOperation::Add, reserve)
    }

    fn elementwise_with_reservation<F>(
        &self,
        rhs: &Self,
        output_limits: LimitsV1,
        operation: ElementwiseOperation,
        reserve: F,
    ) -> Result<Self, ArrayErrorV1>
    where
        F: FnOnce(&mut Vec<f64>, CheckedLayout, ErrorContextV1) -> Result<(), ArrayErrorV1>,
    {
        // Resolve both axes before checking output resources or emptiness.
        let rows = compatible_extent(
            self.shape[0],
            rhs.shape[0],
            0,
            operation.compatibility_context(),
        )?;
        let columns = compatible_extent(
            self.shape[1],
            rhs.shape[1],
            1,
            operation.compatibility_context(),
        )?;
        let shape = [rows, columns];
        let context = operation.output_context();
        let layout = checked_layout(shape, output_limits, context)?;
        let mut data = Vec::new();
        if layout.len == 0 {
            // In particular, do not iterate a huge nonzero extent of an empty
            // shape, read either operand payload, or invoke reservation.
            return Ok(Self { shape, data });
        }
        reserve(&mut data, layout, context)?;

        // Each extent fits usize on the declared 64-bit target. For a nonempty
        // output, compatibility and valid input layouts keep every input base
        // and offset below its input element count, hence representable.
        let rows = rows as usize;
        let columns = columns as usize;
        let lhs_rows = self.shape[0] as usize;
        let rhs_rows = rhs.shape[0] as usize;
        for column in 0..columns {
            let lhs_column = if self.shape[1] == 1 { 0 } else { column };
            let rhs_column = if rhs.shape[1] == 1 { 0 } else { column };
            let lhs_base = lhs_rows * lhs_column;
            let rhs_base = rhs_rows * rhs_column;
            for row in 0..rows {
                let lhs_row = if self.shape[0] == 1 { 0 } else { row };
                let rhs_row = if rhs.shape[0] == 1 { 0 } else { row };
                data.push(
                    operation.apply(self.data[lhs_base + lhs_row], rhs.data[rhs_base + rhs_row]),
                );
            }
        }
        Ok(Self { shape, data })
    }

    /// Reject only the result reservation after ordinary constructor preflight.
    #[cfg(test)]
    pub(crate) fn try_from_col_major_rejecting_reservation(
        shape: [u64; 2],
        source: &[f64],
        limits: LimitsV1,
        attempts: &std::cell::Cell<usize>,
    ) -> Result<Self, ArrayErrorV1> {
        Self::from_col_major_with_reservation(shape, source, limits, |_, layout, context| {
            attempts.set(attempts.get() + 1);
            Err(allocation_failure(layout, context))
        })
    }

    /// Reject only the result reservation after ordinary addition preflight.
    #[cfg(test)]
    pub(crate) fn try_add_rejecting_reservation(
        &self,
        rhs: &Self,
        output_limits: LimitsV1,
        attempts: &std::cell::Cell<usize>,
    ) -> Result<Self, ArrayErrorV1> {
        self.add_with_reservation(rhs, output_limits, |_, layout, context| {
            attempts.set(attempts.get() + 1);
            Err(allocation_failure(layout, context))
        })
    }
}

fn compatible_extent(
    lhs: u64,
    rhs: u64,
    axis: usize,
    context: ErrorContextV1,
) -> Result<u64, ArrayErrorV1> {
    if lhs == rhs {
        Ok(lhs)
    } else if lhs == 1 {
        Ok(rhs)
    } else if rhs == 1 {
        Ok(lhs)
    } else {
        Err(ArrayErrorV1 {
            context,
            code: ErrorCodeV1::IncompatibleShape {
                axis,
                lhs_extent: lhs,
                rhs_extent: rhs,
            },
        })
    }
}

fn checked_layout(
    shape: [u64; 2],
    limits: LimitsV1,
    context: ErrorContextV1,
) -> Result<CheckedLayout, ArrayErrorV1> {
    let [rows, columns] = shape;
    let elements = rows.checked_mul(columns).ok_or(ArrayErrorV1 {
        context,
        code: ErrorCodeV1::ShapeProductOverflow { rows, columns },
    })?;
    let payload_bytes = elements
        .checked_mul(ELEMENT_WIDTH_BYTES)
        .ok_or(ArrayErrorV1 {
            context,
            code: ErrorCodeV1::ByteSizeOverflow {
                elements,
                element_width_bytes: ELEMENT_WIDTH_BYTES,
            },
        })?;
    for (axis, actual) in shape.into_iter().enumerate() {
        if actual > limits.max_dimension {
            return Err(ArrayErrorV1 {
                context,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis,
                    actual,
                    limit: limits.max_dimension,
                },
            });
        }
    }
    if elements > limits.max_elements {
        return Err(ArrayErrorV1 {
            context,
            code: ErrorCodeV1::ElementLimitExceeded {
                actual: elements,
                limit: limits.max_elements,
            },
        });
    }
    if payload_bytes > limits.max_bytes {
        return Err(ArrayErrorV1 {
            context,
            code: ErrorCodeV1::ByteLimitExceeded {
                actual: payload_bytes,
                limit: limits.max_bytes,
            },
        });
    }
    let address_error = ArrayErrorV1 {
        context,
        code: ErrorCodeV1::AddressSpaceExceeded {
            elements,
            payload_bytes,
            pointer_width_bits: usize::BITS,
        },
    };
    let len = usize::try_from(elements).map_err(|_| address_error)?;
    if payload_bytes > isize::MAX as u64 {
        return Err(address_error);
    }
    Ok(CheckedLayout {
        elements,
        payload_bytes,
        len,
    })
}

fn allocation_failure(layout: CheckedLayout, context: ErrorContextV1) -> ArrayErrorV1 {
    ArrayErrorV1 {
        context,
        code: ErrorCodeV1::AllocationFailure {
            requested_elements: layout.elements,
            requested_bytes: layout.payload_bytes,
        },
    }
}

fn reserve_payload(
    data: &mut Vec<f64>,
    layout: CheckedLayout,
    context: ErrorContextV1,
) -> Result<(), ArrayErrorV1> {
    data.try_reserve_exact(layout.len)
        .map_err(|_| allocation_failure(layout, context))
}
