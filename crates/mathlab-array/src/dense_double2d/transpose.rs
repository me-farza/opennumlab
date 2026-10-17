// SPDX-License-Identifier: GPL-3.0-or-later
//! Fallible, real, nonconjugating transpose with independent output storage.

use super::{
    checked_layout, reserve_payload, ArrayErrorV1, CheckedLayout, DenseDouble2d, ErrorContextV1,
    LimitsV1,
};

impl DenseDouble2d {
    /// Swap the two extents and copy each element without floating-point arithmetic.
    ///
    /// The output has independent column-major storage. Output limits and fallible
    /// reservation apply before copying; the source is unchanged on every path.
    pub fn try_transpose(&self, output_limits: LimitsV1) -> Result<Self, ArrayErrorV1> {
        self.transpose_with_reservation(output_limits, reserve_payload)
    }

    fn transpose_with_reservation<F>(
        &self,
        output_limits: LimitsV1,
        reserve: F,
    ) -> Result<Self, ArrayErrorV1>
    where
        F: FnOnce(&mut Vec<f64>, CheckedLayout, ErrorContextV1) -> Result<(), ArrayErrorV1>,
    {
        let shape = [self.shape[1], self.shape[0]];
        let context = ErrorContextV1::TransposeOutput;
        let layout = checked_layout(shape, output_limits, context)?;
        let mut data = Vec::new();
        if layout.len == 0 {
            return Ok(Self { shape, data });
        }
        reserve(&mut data, layout, context)?;

        // Nonempty checked layouts make each extent and source offset fit usize.
        // An output column is one source row, read across source columns.
        let rows = self.shape[0] as usize;
        let columns = self.shape[1] as usize;
        for row in 0..rows {
            for column in 0..columns {
                data.push(self.data[column * rows + row]);
            }
        }
        Ok(Self { shape, data })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{allocation_failure, ErrorCodeV1};
    use super::*;
    use std::cell::Cell;

    const LIMITS: LimitsV1 = LimitsV1 {
        max_dimension: 16,
        max_elements: 256,
        max_bytes: 2048,
    };

    fn array(shape: [u64; 2], values: &[f64]) -> DenseDouble2d {
        DenseDouble2d::try_from_col_major(shape, values, LIMITS).unwrap()
    }

    fn bits(array: &DenseDouble2d) -> Vec<u64> {
        array
            .as_slice()
            .iter()
            .map(|value| value.to_bits())
            .collect()
    }

    #[test]
    fn rectangular_transpose_has_column_major_order_and_independent_ownership() {
        let source = array([2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
        let source_pointer = source.as_slice().as_ptr();
        let result = source.try_transpose(LIMITS).unwrap();
        assert_eq!(result.shape(), [3, 2]);
        assert_eq!(result.as_slice(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_ne!(result.as_slice().as_ptr(), source_pointer);
        assert_eq!(source.shape(), [2, 3]);
        assert_eq!(source.as_slice(), &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
        assert_eq!(source.as_slice().as_ptr(), source_pointer);
        drop(source);
        assert_eq!(result.as_slice(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn square_and_scalar_transposes_copy_into_independent_storage() {
        let source = array([2, 2], &[1.0, 3.0, 2.0, 4.0]);
        let result = source.try_transpose(LIMITS).unwrap();
        assert_eq!(result.shape(), [2, 2]);
        assert_eq!(result.as_slice(), &[1.0, 2.0, 3.0, 4.0]);
        assert_ne!(result.as_slice().as_ptr(), source.as_slice().as_ptr());

        let scalar = array([1, 1], &[-0.0]);
        let result = scalar.try_transpose(LIMITS).unwrap();
        assert_eq!(result.shape(), [1, 1]);
        assert_eq!(bits(&result), [0x8000_0000_0000_0000]);
        assert_ne!(result.as_slice().as_ptr(), scalar.as_slice().as_ptr());
    }

    #[test]
    fn row_and_column_vectors_swap_extents_without_reordering_values() {
        for shape in [[1, 3], [3, 1]] {
            let source = array(shape, &[2.0, -5.0, 9.0]);
            let result = source.try_transpose(LIMITS).unwrap();
            assert_eq!(result.shape(), [shape[1], shape[0]]);
            assert_eq!(result.as_slice(), &[2.0, -5.0, 9.0]);
            assert_ne!(result.as_slice().as_ptr(), source.as_slice().as_ptr());
        }
    }

    #[test]
    fn double_transpose_preserves_shape_order_and_all_payload_bits() {
        let words = [
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0001,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x7ff8_1234_5678_9abc,
            0xfff8_0000_0000_0042,
        ];
        let values = words.map(f64::from_bits);
        let source = array([2, 4], &values);
        let once = source.try_transpose(LIMITS).unwrap();
        assert_eq!(once.shape(), [4, 2]);
        assert_eq!(
            bits(&once),
            [words[0], words[2], words[4], words[6], words[1], words[3], words[5], words[7]]
        );
        let twice = once.try_transpose(LIMITS).unwrap();
        assert_eq!(twice.shape(), [2, 4]);
        assert_eq!(bits(&twice), words);
        assert_eq!(bits(&source), words);
        assert_ne!(source.as_slice().as_ptr(), once.as_slice().as_ptr());
        assert_ne!(source.as_slice().as_ptr(), twice.as_slice().as_ptr());
        assert_ne!(once.as_slice().as_ptr(), twice.as_slice().as_ptr());
        drop(source);
        drop(once);
        assert_eq!(bits(&twice), words);
    }

    #[test]
    fn empty_and_huge_zero_extents_return_before_reservation_or_iteration() {
        let limits = LimitsV1 {
            max_dimension: u64::MAX,
            max_elements: 0,
            max_bytes: 0,
        };
        for shape in [[0, 0], [0, 3], [4, 0], [0, u64::MAX], [u64::MAX, 0]] {
            let source = DenseDouble2d::try_from_col_major(shape, &[], limits).unwrap();
            let result = source
                .transpose_with_reservation(limits, |_, _, _| {
                    panic!("empty transpose must not reserve")
                })
                .unwrap();
            assert_eq!(result.shape(), [shape[1], shape[0]]);
            assert!(result.as_slice().is_empty());
            assert_eq!(source.shape(), shape);
            assert!(source.as_slice().is_empty());
        }
    }

    #[test]
    fn output_dimension_errors_use_transposed_axes_even_for_empty_inputs() {
        let input_limits = LimitsV1 {
            max_dimension: u64::MAX,
            max_elements: 0,
            max_bytes: 0,
        };
        for (shape, axis) in [([0, u64::MAX], 0), ([u64::MAX, 0], 1)] {
            let source = DenseDouble2d::try_from_col_major(shape, &[], input_limits).unwrap();
            let error = source
                .transpose_with_reservation(LIMITS, |_, _, _| {
                    panic!("dimension failure must precede reservation")
                })
                .unwrap_err();
            assert_eq!(
                error,
                ArrayErrorV1 {
                    context: ErrorContextV1::TransposeOutput,
                    code: ErrorCodeV1::DimensionLimitExceeded {
                        axis,
                        actual: u64::MAX,
                        limit: LIMITS.max_dimension,
                    },
                }
            );
            assert_eq!(error.context.as_str(), "transpose.output");
        }
    }

    #[test]
    fn output_preflight_checks_dimensions_then_elements_then_bytes() {
        let source = array([2, 3], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let cases = [
            (
                LimitsV1 {
                    max_dimension: 1,
                    max_elements: 0,
                    max_bytes: 0,
                },
                ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 3,
                    limit: 1,
                },
            ),
            (
                LimitsV1 {
                    max_dimension: 3,
                    max_elements: 5,
                    max_bytes: 0,
                },
                ErrorCodeV1::ElementLimitExceeded {
                    actual: 6,
                    limit: 5,
                },
            ),
            (
                LimitsV1 {
                    max_dimension: 3,
                    max_elements: 6,
                    max_bytes: 47,
                },
                ErrorCodeV1::ByteLimitExceeded {
                    actual: 48,
                    limit: 47,
                },
            ),
        ];
        for (limits, code) in cases {
            let error = source
                .transpose_with_reservation(limits, |_, _, _| {
                    panic!("output preflight must precede reservation")
                })
                .unwrap_err();
            assert_eq!(
                error,
                ArrayErrorV1 {
                    context: ErrorContextV1::TransposeOutput,
                    code
                }
            );
        }
        assert_eq!(source.shape(), [2, 3]);
        assert_eq!(source.as_slice(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn exact_output_limits_allow_the_full_transposed_payload() {
        let source = array([2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
        let limits = LimitsV1 {
            max_dimension: 3,
            max_elements: 6,
            max_bytes: 48,
        };
        let result = source.try_transpose(limits).unwrap();
        assert_eq!(result.shape(), [3, 2]);
        assert_eq!(result.as_slice(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn reservation_failure_reports_output_request_and_preserves_the_source() {
        let source = array([2, 3], &[1.0, -0.0, 3.0, 4.0, 5.0, 6.0]);
        let before = bits(&source);
        let source_pointer = source.as_slice().as_ptr();
        let calls = Cell::new(0);
        let error = source
            .transpose_with_reservation(LIMITS, |data, layout, context| {
                calls.set(calls.get() + 1);
                assert!(data.is_empty());
                assert_eq!(data.capacity(), 0);
                Err(allocation_failure(layout, context))
            })
            .unwrap_err();
        assert_eq!(calls.get(), 1);
        assert_eq!(
            error,
            ArrayErrorV1 {
                context: ErrorContextV1::TransposeOutput,
                code: ErrorCodeV1::AllocationFailure {
                    requested_elements: 6,
                    requested_bytes: 48
                },
            }
        );
        assert_eq!(source.shape(), [2, 3]);
        assert_eq!(source.as_slice().as_ptr(), source_pointer);
        assert_eq!(bits(&source), before);
    }
}
