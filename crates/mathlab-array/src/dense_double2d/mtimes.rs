// SPDX-License-Identifier: GPL-3.0-or-later
//! Draft project policy for owned rank-2 real-double matrix multiplication.
//!
//! Scalar means exactly 1-by-1. Other operands require equal inner extents.
//! Output storage is column-major. Each ordinary dot product visits its inner
//! index in increasing order, starting from positive zero, using separate f64
//! multiplication and addition expressions. No BLAS/FMA/reassociation is used.
//! MATLAB reference, signed-zero/NaN details and numerical/performance parity
//! are unmeasured; the exact-integer test expectations are original project data.

use super::{
    checked_layout, reserve_payload, ArrayErrorV1, CheckedLayout, DenseDouble2d, ErrorContextV1,
    LimitsV1,
};

/// Caller-selected bound on scalar multiplication terms for this one result.
///
/// Ordinary matrix multiplication requires rows * columns * inner_extent terms;
/// scalar multiplication requires one term per output element. Zero inner extent
/// requires zero terms, though filling the output still writes each element.
/// This is not a total-instruction, aggregate-memory or wall-time budget. No
/// default is provided, and no budget value is claimed optimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtimesWorkBudgetV1 {
    pub max_terms: u64,
}

/// Project errors, not native MATLAB identifiers or diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MtimesErrorV1 {
    IncompatibleInnerDimensions {
        lhs_columns: u64,
        rhs_rows: u64,
    },
    WorkCountOverflow {
        output_elements: u64,
        inner_extent: u64,
    },
    WorkLimitExceeded {
        required_terms: u64,
        max_terms: u64,
    },
    /// Unmodified layout/resource/allocation metadata from the array policy.
    Array(ArrayErrorV1),
}

impl From<ArrayErrorV1> for MtimesErrorV1 {
    fn from(value: ArrayErrorV1) -> Self {
        Self::Array(value)
    }
}

fn checked_terms(
    output_elements: u64,
    inner_extent: u64,
    budget: MtimesWorkBudgetV1,
) -> Result<u64, MtimesErrorV1> {
    let required_terms =
        output_elements
            .checked_mul(inner_extent)
            .ok_or(MtimesErrorV1::WorkCountOverflow {
                output_elements,
                inner_extent,
            })?;
    if required_terms > budget.max_terms {
        return Err(MtimesErrorV1::WorkLimitExceeded {
            required_terms,
            max_terms: budget.max_terms,
        });
    }
    Ok(required_terms)
}

impl DenseDouble2d {
    /// Matrix product, or scalar multiplication when either shape is [1,1].
    ///
    /// Error order is inner compatibility, existing output-layout policy,
    /// checked term count/work limit, then fallible output reservation. A zero
    /// output returns after layout validation without reservation, data access,
    /// integer casts or dimension-sized loops. Inputs are immutable; output is
    /// independently owned. No engine registration or alternative backend runs.
    pub fn try_mtimes(
        &self,
        rhs: &Self,
        output_limits: LimitsV1,
        work_budget: MtimesWorkBudgetV1,
    ) -> Result<Self, MtimesErrorV1> {
        self.mtimes_with_reservation(rhs, output_limits, work_budget, reserve_payload)
    }

    fn mtimes_with_reservation<F>(
        &self,
        rhs: &Self,
        output_limits: LimitsV1,
        work_budget: MtimesWorkBudgetV1,
        reserve: F,
    ) -> Result<Self, MtimesErrorV1>
    where
        F: FnOnce(&mut Vec<f64>, CheckedLayout, ErrorContextV1) -> Result<(), ArrayErrorV1>,
    {
        let lhs_scalar = self.shape == [1, 1];
        let rhs_scalar = rhs.shape == [1, 1];
        if !lhs_scalar && !rhs_scalar && self.shape[1] != rhs.shape[0] {
            return Err(MtimesErrorV1::IncompatibleInnerDimensions {
                lhs_columns: self.shape[1],
                rhs_rows: rhs.shape[0],
            });
        }
        let shape = if lhs_scalar {
            rhs.shape
        } else if rhs_scalar {
            self.shape
        } else {
            [self.shape[0], rhs.shape[1]]
        };
        let context = ErrorContextV1::MtimesOutput;
        let layout = checked_layout(shape, output_limits, context)?;
        let mut data = Vec::new();
        if layout.len == 0 {
            return Ok(Self { shape, data });
        }
        let inner_extent = if lhs_scalar || rhs_scalar {
            1
        } else {
            self.shape[1]
        };
        checked_terms(layout.elements, inner_extent, work_budget)?;
        reserve(&mut data, layout, context)?;

        if lhs_scalar {
            let scalar = self.data[0];
            for &value in &rhs.data {
                data.push(scalar * value);
            }
        } else if rhs_scalar {
            let scalar = rhs.data[0];
            for &value in &self.data {
                data.push(value * scalar);
            }
        } else {
            // Nonempty checked output makes both output extents fit usize.
            // For inner_extent > 0, valid input layouts bound each input offset.
            // For inner_extent == 0, neither input payload is ever accessed.
            let rows = shape[0] as usize;
            let columns = shape[1] as usize;
            let inner = inner_extent as usize;
            for column in 0..columns {
                for row in 0..rows {
                    let mut sum = 0.0_f64;
                    for index in 0..inner {
                        let product =
                            self.data[row + rows * index] * rhs.data[index + inner * column];
                        sum += product;
                    }
                    data.push(sum);
                }
            }
        }
        Ok(Self { shape, data })
    }
}

#[cfg(test)]
mod tests;
