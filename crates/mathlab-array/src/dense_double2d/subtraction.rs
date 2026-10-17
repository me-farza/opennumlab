// SPDX-License-Identifier: GPL-3.0-or-later
//! Original project real-double subtraction; no native subtraction oracle yet.

use super::{reserve_payload, ArrayErrorV1, DenseDouble2d, ElementwiseOperation, LimitsV1};

impl DenseDouble2d {
    /// Elementwise `lhs - rhs` with the existing singleton expansion policy.
    ///
    /// Each output uses one direct primitive f64 subtraction. Compatibility is
    /// checked row then column, followed by output layout/limits and fallible
    /// allocation. Empty output returns before reservation, casts or loops.
    /// Inputs stay borrowed and unchanged; the result owns independent storage.
    /// No native signed-zero/NaN, general IEEE or performance parity is claimed.
    pub fn try_sub(&self, rhs: &Self, output_limits: LimitsV1) -> Result<Self, ArrayErrorV1> {
        self.elementwise_with_reservation(
            rhs,
            output_limits,
            ElementwiseOperation::Subtract,
            reserve_payload,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::{allocation_failure, ErrorCodeV1, ErrorContextV1};
    use super::*;
    use std::cell::Cell;

    const LIMITS: LimitsV1 = LimitsV1 {
        max_dimension: 16,
        max_elements: 256,
        max_bytes: 2048,
    };
    fn array(shape: [u64; 2], data: &[f64]) -> DenseDouble2d {
        DenseDouble2d::try_from_col_major(shape, data, LIMITS).unwrap()
    }

    #[test]
    fn original_noncommutative_and_broadcast_results_have_independent_owned_storage() {
        let lhs = array([1, 3], &[10.0, 20.0, 30.0]);
        let rhs = array([2, 1], &[1.0, 2.0]);
        let difference = lhs.try_sub(&rhs, LIMITS).unwrap();
        assert_eq!(difference.shape(), [2, 3]);
        assert_eq!(difference.as_slice(), &[9.0, 8.0, 19.0, 18.0, 29.0, 28.0]);
        assert_ne!(difference.as_slice().as_ptr(), lhs.as_slice().as_ptr());
        assert_ne!(difference.as_slice().as_ptr(), rhs.as_slice().as_ptr());
        let reverse = rhs.try_sub(&lhs, LIMITS).unwrap();
        assert_eq!(
            reverse.as_slice(),
            &[-9.0, -8.0, -19.0, -18.0, -29.0, -28.0]
        );
        assert_eq!(lhs.as_slice(), &[10.0, 20.0, 30.0]);
        assert_eq!(rhs.as_slice(), &[1.0, 2.0]);
        drop((lhs, rhs));
        assert_eq!(difference.as_slice(), &[9.0, 8.0, 19.0, 18.0, 29.0, 28.0]);
        let matrix = array([2, 2], &[1.0, 3.0, 2.0, 4.0]);
        let scalar = array([1, 1], &[5.0]);
        assert_eq!(
            scalar.try_sub(&matrix, LIMITS).unwrap().as_slice(),
            &[4.0, 2.0, 3.0, 1.0]
        );
        assert_eq!(
            matrix.try_sub(&scalar, LIMITS).unwrap().as_slice(),
            &[-4.0, -2.0, -3.0, -1.0]
        );
    }

    #[test]
    fn original_direct_subtraction_signed_zero_words_are_explicit() {
        let lhs = array([1, 4], &[0.0, 0.0, -0.0, -0.0]);
        let rhs = array([1, 4], &[0.0, -0.0, 0.0, -0.0]);
        let result = lhs.try_sub(&rhs, LIMITS).unwrap();
        let expected = [
            0x0000000000000000_u64,
            0x0000000000000000,
            0x8000000000000000,
            0x0000000000000000,
        ];
        for (element, bits) in result.as_slice().iter().zip(expected) {
            assert_eq!(element.to_bits(), bits);
        }
    }

    #[test]
    fn empty_axis_expansion_keeps_shape_and_skips_reservation() {
        let cases = [
            ([0, 0], [1, 1], [0, 0]),
            ([1, 1], [0, 0], [0, 0]),
            ([0, 3], [1, 3], [0, 3]),
            ([0, 1], [1, 4], [0, 4]),
            ([1, 0], [3, 1], [3, 0]),
        ];
        for (lhs_shape, rhs_shape, expected_shape) in cases {
            let lhs_data = vec![1.0; (lhs_shape[0] * lhs_shape[1]) as usize];
            let rhs_data = vec![2.0; (rhs_shape[0] * rhs_shape[1]) as usize];
            let lhs = array(lhs_shape, &lhs_data);
            let rhs = array(rhs_shape, &rhs_data);
            let result = lhs
                .elementwise_with_reservation(
                    &rhs,
                    LIMITS,
                    ElementwiseOperation::Subtract,
                    |_, _, _| panic!("empty subtraction must not reserve"),
                )
                .unwrap();
            assert_eq!(result.shape(), expected_shape);
            assert!(result.as_slice().is_empty());
        }
        let limits = LimitsV1 {
            max_dimension: u64::MAX,
            max_elements: 1,
            max_bytes: 8,
        };
        let empty = DenseDouble2d::try_from_col_major([0, u64::MAX], &[], limits).unwrap();
        let scalar = DenseDouble2d::try_from_col_major([1, 1], &[1.0], limits).unwrap();
        for result in [
            empty.try_sub(&scalar, limits),
            scalar.try_sub(&empty, limits),
        ] {
            let result = result.unwrap();
            assert_eq!(result.shape(), [0, u64::MAX]);
            assert!(result.as_slice().is_empty());
        }
    }

    #[test]
    fn both_axes_are_checked_before_output_limits_and_empty_shortcuts() {
        let lhs = array([2, 2], &[1.0, 2.0, 3.0, 4.0]);
        let rhs = array([3, 3], &[1.0; 9]);
        let zero = LimitsV1 {
            max_dimension: 0,
            max_elements: 0,
            max_bytes: 0,
        };
        assert_eq!(
            lhs.try_sub(&rhs, zero).unwrap_err(),
            ArrayErrorV1 {
                context: ErrorContextV1::SubCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2,
                    rhs_extent: 3
                },
            }
        );
        let lhs = array([0, 2], &[]);
        let rhs = array([0, 3], &[]);
        assert_eq!(
            lhs.try_sub(&rhs, zero).unwrap_err(),
            ArrayErrorV1 {
                context: ErrorContextV1::SubCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 1,
                    lhs_extent: 2,
                    rhs_extent: 3
                },
            }
        );
        assert_eq!(
            ErrorContextV1::SubCompatibility.as_str(),
            "sub.compatibility"
        );
        assert_eq!(ErrorContextV1::SubOutput.as_str(), "sub.output");
    }

    #[test]
    fn output_dimension_element_and_byte_limits_keep_the_shared_order() {
        let lhs = array([2, 1], &[1.0, 2.0]);
        let rhs = array([1, 3], &[10.0, 20.0, 30.0]);
        for (limits, expected) in [
            (
                LimitsV1 {
                    max_dimension: 1,
                    max_elements: 0,
                    max_bytes: 0,
                },
                ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 2,
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
        ] {
            assert_eq!(
                lhs.try_sub(&rhs, limits).unwrap_err(),
                ArrayErrorV1 {
                    context: ErrorContextV1::SubOutput,
                    code: expected,
                }
            );
        }
        let exact = LimitsV1 {
            max_dimension: 3,
            max_elements: 6,
            max_bytes: 48,
        };
        assert_eq!(
            lhs.try_sub(&rhs, exact).unwrap().as_slice(),
            &[-9.0, -8.0, -19.0, -18.0, -29.0, -28.0]
        );
    }

    #[test]
    fn allocation_rejection_follows_preflight_and_retains_subtraction_metadata() {
        let lhs = array([2, 1], &[1.0, 2.0]);
        let rhs = array([1, 3], &[10.0, 20.0, 30.0]);
        let attempts = Cell::new(0);
        let failure = lhs
            .elementwise_with_reservation(
                &rhs,
                LIMITS,
                ElementwiseOperation::Subtract,
                |_, layout, context| {
                    attempts.set(attempts.get() + 1);
                    Err(allocation_failure(layout, context))
                },
            )
            .unwrap_err();
        assert_eq!(attempts.get(), 1);
        assert_eq!(
            failure,
            ArrayErrorV1 {
                context: ErrorContextV1::SubOutput,
                code: ErrorCodeV1::AllocationFailure {
                    requested_elements: 6,
                    requested_bytes: 48
                }
            }
        );
        assert_eq!(lhs.as_slice(), &[1.0, 2.0]);
        assert_eq!(rhs.as_slice(), &[10.0, 20.0, 30.0]);
    }
}
