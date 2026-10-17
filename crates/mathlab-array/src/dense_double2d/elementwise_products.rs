// SPDX-License-Identifier: GPL-3.0-or-later
//! Direct binary64 elementwise product/quotient under the shared array policy.
//! New project cases do not claim a native array-operation oracle.

use super::{reserve_payload, ArrayErrorV1, DenseDouble2d, ElementwiseOperation, LimitsV1};

impl DenseDouble2d {
    /// Elementwise lhs*rhs with singleton expansion and independent owned output.
    /// Compatibility, output limits, emptiness and reservation follow try_add.
    /// One direct primitive multiplication is used per cell; zero/nonfinite
    /// inputs are admitted. No NaN payload or broad native parity is promised.
    pub fn try_times(&self, rhs: &Self, output_limits: LimitsV1) -> Result<Self, ArrayErrorV1> {
        self.elementwise_with_reservation(
            rhs,
            output_limits,
            ElementwiseOperation::Times,
            reserve_payload,
        )
    }

    /// Elementwise lhs/rhs with singleton expansion and independent owned output.
    /// Zero divisors are admitted: primitive f64 division can produce signed
    /// infinity or NaN. No invented division-by-zero diagnostic or matrix solve
    /// is introduced. Other array checks/order are exactly the shared policy.
    pub fn try_rdivide(&self, rhs: &Self, output_limits: LimitsV1) -> Result<Self, ArrayErrorV1> {
        self.elementwise_with_reservation(
            rhs,
            output_limits,
            ElementwiseOperation::RDivide,
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
    fn original_broadcast_products_and_noncommutative_quotients_are_owned() {
        let column = array([2, 1], &[2.0, 4.0]);
        let row = array([1, 3], &[2.0, 4.0, 8.0]);
        let product = column.try_times(&row, LIMITS).unwrap();
        assert_eq!(product.shape(), [2, 3]);
        assert_eq!(product.as_slice(), &[4.0, 8.0, 8.0, 16.0, 16.0, 32.0]);
        assert_ne!(product.as_slice().as_ptr(), column.as_slice().as_ptr());
        assert_ne!(product.as_slice().as_ptr(), row.as_slice().as_ptr());
        let quotient = column.try_rdivide(&row, LIMITS).unwrap();
        assert_eq!(quotient.shape(), [2, 3]);
        assert_eq!(quotient.as_slice(), &[1.0, 2.0, 0.5, 1.0, 0.25, 0.5]);
        assert_eq!(
            row.try_rdivide(&column, LIMITS).unwrap().as_slice(),
            &[1.0, 0.5, 2.0, 1.0, 4.0, 2.0]
        );
        let scalar = array([1, 1], &[8.0]);
        assert_eq!(
            scalar.try_rdivide(&column, LIMITS).unwrap().as_slice(),
            &[4.0, 2.0]
        );
        assert_eq!(
            column.try_rdivide(&scalar, LIMITS).unwrap().as_slice(),
            &[0.25, 0.5]
        );
        assert_eq!(column.as_slice(), &[2.0, 4.0]);
        assert_eq!(row.as_slice(), &[2.0, 4.0, 8.0]);
        drop((column, row));
        assert_eq!(product.as_slice(), &[4.0, 8.0, 8.0, 16.0, 16.0, 32.0]);
        assert_eq!(quotient.as_slice(), &[1.0, 2.0, 0.5, 1.0, 0.25, 0.5]);
    }

    #[test]
    fn original_zero_and_nonfinite_policy_checks_words_or_nan_category() {
        let zeros = array([1, 4], &[0.0, -0.0, 0.0, -0.0]);
        let signs = array([1, 4], &[2.0, 2.0, -2.0, -2.0]);
        for result in [
            zeros.try_times(&signs, LIMITS),
            zeros.try_rdivide(&signs, LIMITS),
        ] {
            let result = result.unwrap();
            let expected = [
                0x0000000000000000_u64,
                0x8000000000000000,
                0x8000000000000000,
                0x0000000000000000,
            ];
            for (value, word) in result.as_slice().iter().zip(expected) {
                assert_eq!(value.to_bits(), word);
            }
        }
        let finite = array([1, 4], &[1.0, -1.0, 1.0, -1.0]);
        let divisors = array([1, 4], &[0.0, 0.0, -0.0, -0.0]);
        let result = finite.try_rdivide(&divisors, LIMITS).unwrap();
        let expected = [
            0x7ff0000000000000_u64,
            0xfff0000000000000,
            0xfff0000000000000,
            0x7ff0000000000000,
        ];
        for (value, word) in result.as_slice().iter().zip(expected) {
            assert_eq!(value.to_bits(), word);
        }
        let zero = array([1, 1], &[0.0]);
        let infinity = array([1, 1], &[f64::INFINITY]);
        assert!(zero.try_times(&infinity, LIMITS).unwrap().as_slice()[0].is_nan());
        assert!(zero.try_rdivide(&zero, LIMITS).unwrap().as_slice()[0].is_nan());
        assert!(infinity.try_rdivide(&infinity, LIMITS).unwrap().as_slice()[0].is_nan());
    }

    #[test]
    fn empty_axes_and_huge_empty_shapes_skip_reservation_and_extent_loops() {
        let empty = array([0, 1], &[]);
        let row = array([1, 4], &[1.0, 2.0, 3.0, 4.0]);
        for operation in [ElementwiseOperation::Times, ElementwiseOperation::RDivide] {
            let value = empty
                .elementwise_with_reservation(&row, LIMITS, operation, |_, _, _| {
                    panic!("zero output must not reserve")
                })
                .unwrap();
            assert_eq!(value.shape(), [0, 4]);
            assert!(value.as_slice().is_empty());
        }
        let limits = LimitsV1 {
            max_dimension: u64::MAX,
            max_elements: 1,
            max_bytes: 8,
        };
        let empty = DenseDouble2d::try_from_col_major([u64::MAX, 0], &[], limits).unwrap();
        let scalar = DenseDouble2d::try_from_col_major([1, 1], &[1.0], limits).unwrap();
        for value in [
            empty.try_times(&scalar, limits),
            scalar.try_times(&empty, limits),
            empty.try_rdivide(&scalar, limits),
            scalar.try_rdivide(&empty, limits),
        ] {
            let value = value.unwrap();
            assert_eq!(value.shape(), [u64::MAX, 0]);
            assert!(value.as_slice().is_empty());
        }
    }

    #[test]
    fn compatibility_checks_precede_limits_and_empty_shortcuts_with_own_contexts() {
        let lhs = array([2, 2], &[1.0; 4]);
        let rhs = array([3, 3], &[2.0; 9]);
        let limits = LimitsV1 {
            max_dimension: 0,
            max_elements: 0,
            max_bytes: 0,
        };
        for (operation, context) in [
            (
                ElementwiseOperation::Times,
                ErrorContextV1::TimesCompatibility,
            ),
            (
                ElementwiseOperation::RDivide,
                ErrorContextV1::RDivideCompatibility,
            ),
        ] {
            let failure = lhs
                .elementwise_with_reservation(&rhs, limits, operation, |_, _, _| {
                    panic!("compatibility failure must not reserve")
                })
                .unwrap_err();
            assert_eq!(
                failure,
                ArrayErrorV1 {
                    context,
                    code: ErrorCodeV1::IncompatibleShape {
                        axis: 0,
                        lhs_extent: 2,
                        rhs_extent: 3
                    }
                }
            );
            let lhs = array([0, 2], &[]);
            let rhs = array([0, 3], &[]);
            let failure = lhs
                .elementwise_with_reservation(&rhs, limits, operation, |_, _, _| {
                    panic!("empty incompatibility must not reserve")
                })
                .unwrap_err();
            assert_eq!(
                failure,
                ArrayErrorV1 {
                    context,
                    code: ErrorCodeV1::IncompatibleShape {
                        axis: 1,
                        lhs_extent: 2,
                        rhs_extent: 3
                    }
                }
            );
        }
    }

    #[test]
    fn output_limits_and_labels_are_specific_to_the_selected_operation() {
        let lhs = array([2, 1], &[2.0, 4.0]);
        let rhs = array([1, 3], &[2.0, 4.0, 8.0]);
        for (operation, context) in [
            (ElementwiseOperation::Times, ErrorContextV1::TimesOutput),
            (ElementwiseOperation::RDivide, ErrorContextV1::RDivideOutput),
        ] {
            for (limits, code) in [
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
                let failure = lhs
                    .elementwise_with_reservation(&rhs, limits, operation, |_, _, _| {
                        panic!("layout failure must not reserve")
                    })
                    .unwrap_err();
                assert_eq!(failure, ArrayErrorV1 { context, code });
            }
        }
        assert_eq!(
            ErrorContextV1::TimesCompatibility.as_str(),
            "times.compatibility"
        );
        assert_eq!(ErrorContextV1::TimesOutput.as_str(), "times.output");
        assert_eq!(
            ErrorContextV1::RDivideCompatibility.as_str(),
            "rdivide.compatibility"
        );
        assert_eq!(ErrorContextV1::RDivideOutput.as_str(), "rdivide.output");
    }

    #[test]
    fn reservation_rejection_retains_complete_operation_metadata_and_inputs() {
        let lhs = array([2, 1], &[2.0, 4.0]);
        let rhs = array([1, 3], &[2.0, 4.0, 8.0]);
        for (operation, context) in [
            (ElementwiseOperation::Times, ErrorContextV1::TimesOutput),
            (ElementwiseOperation::RDivide, ErrorContextV1::RDivideOutput),
        ] {
            let attempts = Cell::new(0);
            let failure = lhs
                .elementwise_with_reservation(
                    &rhs,
                    LIMITS,
                    operation,
                    |_, layout, received_context| {
                        attempts.set(attempts.get() + 1);
                        Err(allocation_failure(layout, received_context))
                    },
                )
                .unwrap_err();
            assert_eq!(attempts.get(), 1);
            assert_eq!(
                failure,
                ArrayErrorV1 {
                    context,
                    code: ErrorCodeV1::AllocationFailure {
                        requested_elements: 6,
                        requested_bytes: 48
                    }
                }
            );
        }
        assert_eq!(lhs.as_slice(), &[2.0, 4.0]);
        assert_eq!(rhs.as_slice(), &[2.0, 4.0, 8.0]);
    }
}
