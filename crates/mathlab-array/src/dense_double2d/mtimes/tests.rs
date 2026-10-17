// SPDX-License-Identifier: GPL-3.0-or-later
//! Original project expectations plus the two explicitly identified, retained
//! MATLAB numerical observations below; no new reference runtime is invoked.

use super::super::{allocation_failure, ErrorCodeV1};
use super::*;
use std::cell::Cell;

const LIMITS: LimitsV1 = LimitsV1 {
    max_dimension: 16,
    max_elements: 256,
    max_bytes: 2048,
};

const UNBOUNDED_LAYOUT: LimitsV1 = LimitsV1 {
    max_dimension: u64::MAX,
    max_elements: u64::MAX,
    max_bytes: u64::MAX,
};

fn budget(max_terms: u64) -> MtimesWorkBudgetV1 {
    MtimesWorkBudgetV1 { max_terms }
}

fn array(shape: [u64; 2], data: &[f64]) -> DenseDouble2d {
    DenseDouble2d::try_from_col_major(shape, data, LIMITS).unwrap()
}

fn empty(shape: [u64; 2]) -> DenseDouble2d {
    DenseDouble2d::try_from_col_major(shape, &[], UNBOUNDED_LAYOUT).unwrap()
}

fn assert_array(value: &DenseDouble2d, shape: [u64; 2], column_major: &[f64]) {
    assert_eq!(value.shape(), shape);
    assert_eq!(value.as_slice(), column_major);
}

#[test]
fn selected_native_b02_square_and_b03_empty_products_match() {
    // Retained selected99 B:B02/B:B03, backend-20261015T062102198338Z.
    // run_backend_probes.m observes M=[1,2;3,4]; M*M=[7,10;15,22],
    // and zeros(0,2)*ones(2,4) with double shape 0-by-4. These identical
    // operands test numerical cases only, not whole-source or BLAS parity.
    let matrix = array([2, 2], &[1.0, 3.0, 2.0, 4.0]);
    let square = matrix.try_mtimes(&matrix, LIMITS, budget(8)).unwrap();
    assert_array(&square, [2, 2], &[7.0, 15.0, 10.0, 22.0]);

    let lhs = empty([0, 2]);
    let rhs = array([2, 4], &[1.0; 8]);
    let product = lhs.try_mtimes(&rhs, LIMITS, budget(0)).unwrap();
    assert_array(&product, [0, 4], &[]);
}

#[test]
fn scalar_product_and_both_scaling_directions_are_explicit() {
    let two = array([1, 1], &[2.0]);
    let three = array([1, 1], &[3.0]);
    assert_array(
        &two.try_mtimes(&three, LIMITS, budget(1)).unwrap(),
        [1, 1],
        &[6.0],
    );

    // Rows [1,-2;4,3] times scalar -2 => rows [-2,4;-8,-6].
    let scalar = array([1, 1], &[-2.0]);
    let matrix = array([2, 2], &[1.0, 4.0, -2.0, 3.0]);
    let left = scalar.try_mtimes(&matrix, LIMITS, budget(4)).unwrap();
    let right = matrix.try_mtimes(&scalar, LIMITS, budget(4)).unwrap();
    assert_array(&left, [2, 2], &[-2.0, -8.0, 4.0, -6.0]);
    assert_array(&right, [2, 2], &[-2.0, -8.0, 4.0, -6.0]);
    assert_array(&matrix, [2, 2], &[1.0, 4.0, -2.0, 3.0]);
    assert_array(&scalar, [1, 1], &[-2.0]);
}

#[test]
fn rectangular_product_preserves_column_major_output() {
    // A=[1,2,3;4,5,6], B=[7,8;9,10;11,12].
    // AB=[58,64;139,154], each entry a three-term integer dot product.
    let lhs = array([2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    let rhs = array([3, 2], &[7.0, 9.0, 11.0, 8.0, 10.0, 12.0]);
    let result = lhs.try_mtimes(&rhs, LIMITS, budget(12)).unwrap();
    assert_array(&result, [2, 2], &[58.0, 139.0, 64.0, 154.0]);
    drop(lhs);
    drop(rhs);
    assert_array(&result, [2, 2], &[58.0, 139.0, 64.0, 154.0]);
}

#[test]
fn multiplication_does_not_commute() {
    // A=[1,2;0,1], B=[1,0;3,1].
    let lhs = array([2, 2], &[1.0, 0.0, 2.0, 1.0]);
    let rhs = array([2, 2], &[1.0, 3.0, 0.0, 1.0]);
    assert_array(
        &lhs.try_mtimes(&rhs, LIMITS, budget(8)).unwrap(),
        [2, 2],
        &[7.0, 3.0, 2.0, 1.0],
    );
    assert_array(
        &rhs.try_mtimes(&lhs, LIMITS, budget(8)).unwrap(),
        [2, 2],
        &[1.0, 3.0, 2.0, 7.0],
    );
}

#[test]
fn row_column_dot_and_column_row_outer_products_are_distinct() {
    let row = array([1, 3], &[1.0, -2.0, 3.0]);
    let column = array([3, 1], &[4.0, 5.0, -6.0]);
    // 1*4 + (-2)*5 + 3*(-6) = -24.
    assert_array(
        &row.try_mtimes(&column, LIMITS, budget(3)).unwrap(),
        [1, 1],
        &[-24.0],
    );
    let column = array([3, 1], &[2.0, -3.0, 4.0]);
    let row = array([1, 2], &[-1.0, 5.0]);
    assert_array(
        &column.try_mtimes(&row, LIMITS, budget(6)).unwrap(),
        [3, 2],
        &[-2.0, 3.0, -4.0, 10.0, -15.0, 20.0],
    );
}

#[test]
fn scalar_empty_products_preserve_every_empty_shape_without_reservation() {
    let scalar = array([1, 1], &[-2.0]);
    for shape in [[0, 0], [0, 4], [5, 0]] {
        let value = empty(shape);
        let attempts = Cell::new(0);
        for (lhs, rhs) in [(&scalar, &value), (&value, &scalar)] {
            let result = lhs
                .mtimes_with_reservation(rhs, LIMITS, budget(0), |_, layout, context| {
                    attempts.set(attempts.get() + 1);
                    Err(allocation_failure(layout, context))
                })
                .unwrap();
            assert_array(&result, shape, &[]);
        }
        assert_eq!(attempts.get(), 0);
    }
}

#[test]
fn zero_inner_dimension_constructs_the_full_zero_output_with_zero_terms() {
    let lhs = empty([2, 0]);
    let rhs = empty([0, 3]);
    let attempts = Cell::new(0);
    let result = lhs
        .mtimes_with_reservation(&rhs, LIMITS, budget(0), |data, layout, context| {
            attempts.set(attempts.get() + 1);
            reserve_payload(data, layout, context)
        })
        .unwrap();
    assert_array(&result, [2, 3], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(attempts.get(), 1);
}

#[test]
fn enormous_empty_dimensions_return_without_dimension_sized_loops_or_allocation() {
    let lhs = empty([0, u64::MAX]);
    let rhs = empty([u64::MAX, 0]);
    let attempts = Cell::new(0);
    let result = lhs
        .mtimes_with_reservation(
            &rhs,
            LimitsV1 {
                max_dimension: 0,
                max_elements: 0,
                max_bytes: 0,
            },
            budget(0),
            |_, layout, context| {
                attempts.set(attempts.get() + 1);
                Err(allocation_failure(layout, context))
            },
        )
        .unwrap();
    assert_array(&result, [0, 0], &[]);
    assert_eq!(attempts.get(), 0);
}

#[test]
fn incompatible_inner_dimensions_precede_output_and_work_limits_even_for_empty_arrays() {
    let lhs = empty([0, 2]);
    let rhs = empty([3, 0]);
    let attempts = Cell::new(0);
    let error = lhs
        .mtimes_with_reservation(
            &rhs,
            LimitsV1 {
                max_dimension: 0,
                max_elements: 0,
                max_bytes: 0,
            },
            budget(0),
            |_, layout, context| {
                attempts.set(attempts.get() + 1);
                Err(allocation_failure(layout, context))
            },
        )
        .unwrap_err();
    assert_eq!(
        error,
        MtimesErrorV1::IncompatibleInnerDimensions {
            lhs_columns: 2,
            rhs_rows: 3
        }
    );
    assert_eq!(attempts.get(), 0);
    let lhs = array([2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    let rhs = array([2, 2], &[1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        lhs.try_mtimes(&rhs, LIMITS, budget(100)).unwrap_err(),
        MtimesErrorV1::IncompatibleInnerDimensions {
            lhs_columns: 3,
            rhs_rows: 2
        }
    );
}

#[test]
fn work_budget_counts_terms_and_rejects_before_reservation() {
    let lhs = array([2, 3], &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]);
    let rhs = array([3, 2], &[7.0, 9.0, 11.0, 8.0, 10.0, 12.0]);
    let attempts = Cell::new(0);
    let error = lhs
        .mtimes_with_reservation(&rhs, LIMITS, budget(11), |_, layout, context| {
            attempts.set(attempts.get() + 1);
            Err(allocation_failure(layout, context))
        })
        .unwrap_err();
    assert_eq!(
        error,
        MtimesErrorV1::WorkLimitExceeded {
            required_terms: 12,
            max_terms: 11
        }
    );
    assert_eq!(attempts.get(), 0);
    assert_array(
        &lhs.try_mtimes(&rhs, LIMITS, budget(12)).unwrap(),
        [2, 2],
        &[58.0, 139.0, 64.0, 154.0],
    );
    let scalar = array([1, 1], &[2.0]);
    assert_eq!(
        scalar.try_mtimes(&rhs, LIMITS, budget(5)).unwrap_err(),
        MtimesErrorV1::WorkLimitExceeded {
            required_terms: 6,
            max_terms: 5
        }
    );
    assert_array(
        &scalar.try_mtimes(&rhs, LIMITS, budget(6)).unwrap(),
        [3, 2],
        &[14.0, 18.0, 22.0, 16.0, 20.0, 24.0],
    );
}

#[test]
fn work_count_overflow_is_distinct_from_work_limit_or_allocation() {
    assert_eq!(
        checked_terms(9_000_000_000_000, 3_000_000, budget(u64::MAX)).unwrap_err(),
        MtimesErrorV1::WorkCountOverflow {
            output_elements: 9_000_000_000_000,
            inner_extent: 3_000_000
        }
    );
    assert_eq!(
        checked_terms(u64::MAX, 1, budget(u64::MAX)).unwrap(),
        u64::MAX
    );
    assert_eq!(checked_terms(0, u64::MAX, budget(0)).unwrap(), 0);
}

#[test]
fn existing_output_layout_errors_precede_work_checks_and_keep_complete_metadata() {
    let lhs = array([2, 2], &[1.0, 0.0, 2.0, 1.0]);
    let rhs = array([2, 2], &[1.0, 3.0, 0.0, 1.0]);
    for (limits, code) in [
        (
            LimitsV1 {
                max_dimension: 1,
                ..LIMITS
            },
            ErrorCodeV1::DimensionLimitExceeded {
                axis: 0,
                actual: 2,
                limit: 1,
            },
        ),
        (
            LimitsV1 {
                max_elements: 3,
                ..LIMITS
            },
            ErrorCodeV1::ElementLimitExceeded {
                actual: 4,
                limit: 3,
            },
        ),
        (
            LimitsV1 {
                max_bytes: 31,
                ..LIMITS
            },
            ErrorCodeV1::ByteLimitExceeded {
                actual: 32,
                limit: 31,
            },
        ),
    ] {
        assert_eq!(
            lhs.try_mtimes(&rhs, limits, budget(0)).unwrap_err(),
            MtimesErrorV1::Array(ArrayErrorV1 {
                context: ErrorContextV1::MtimesOutput,
                code
            })
        );
    }
    assert_eq!(ErrorContextV1::MtimesOutput.as_str(), "mtimes.output");
}

#[test]
fn zero_inner_operands_allow_safe_testing_of_output_arithmetic_failures_without_payloads() {
    for (rows, columns, code) in [
        (
            u64::MAX,
            2,
            ErrorCodeV1::ShapeProductOverflow {
                rows: u64::MAX,
                columns: 2,
            },
        ),
        (
            u64::MAX / 8 + 1,
            1,
            ErrorCodeV1::ByteSizeOverflow {
                elements: u64::MAX / 8 + 1,
                element_width_bytes: 8,
            },
        ),
        (
            1_u64 << 60,
            1,
            ErrorCodeV1::AddressSpaceExceeded {
                elements: 1_u64 << 60,
                payload_bytes: 1_u64 << 63,
                pointer_width_bits: usize::BITS,
            },
        ),
    ] {
        let lhs = empty([rows, 0]);
        let rhs = empty([0, columns]);
        let attempts = Cell::new(0);
        assert_eq!(
            lhs.mtimes_with_reservation(
                &rhs,
                UNBOUNDED_LAYOUT,
                budget(u64::MAX),
                |_, layout, context| {
                    attempts.set(attempts.get() + 1);
                    Err(allocation_failure(layout, context))
                }
            )
            .unwrap_err(),
            MtimesErrorV1::Array(ArrayErrorV1 {
                context: ErrorContextV1::MtimesOutput,
                code
            })
        );
        assert_eq!(attempts.get(), 0);
    }
}

#[test]
fn empty_output_still_obeys_existing_dimension_limits() {
    let lhs = empty([0, 0]);
    let rhs = empty([0, u64::MAX]);
    assert_eq!(
        lhs.try_mtimes(&rhs, LIMITS, budget(0)).unwrap_err(),
        MtimesErrorV1::Array(ArrayErrorV1 {
            context: ErrorContextV1::MtimesOutput,
            code: ErrorCodeV1::DimensionLimitExceeded {
                axis: 1,
                actual: u64::MAX,
                limit: 16
            },
        })
    );
}

#[test]
fn rejected_result_reservation_is_fallible_and_retains_its_output_request() {
    let lhs = empty([2, 0]);
    let rhs = empty([0, 3]);
    let attempts = Cell::new(0);
    let error = lhs
        .mtimes_with_reservation(&rhs, LIMITS, budget(0), |_, layout, context| {
            attempts.set(attempts.get() + 1);
            Err(allocation_failure(layout, context))
        })
        .unwrap_err();
    assert_eq!(
        error,
        MtimesErrorV1::Array(ArrayErrorV1 {
            context: ErrorContextV1::MtimesOutput,
            code: ErrorCodeV1::AllocationFailure {
                requested_elements: 6,
                requested_bytes: 48
            },
        })
    );
    assert_eq!(attempts.get(), 1);
    assert_array(&lhs, [2, 0], &[]);
    assert_array(&rhs, [0, 3], &[]);
}
