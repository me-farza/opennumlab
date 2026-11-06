// SPDX-License-Identifier: GPL-3.0-or-later
//! Standalone original RN-PREP-01 tests, not a MATLAB-language execution suite.
//! Expected shapes/values/errors below come from the reviewed contract, not the implementation.
//! Contract SHA-256: 3086b3f06e6dfeb7fc3733dd497a99f3a73ce76b00f95e37c357dd9c7b936b59
#![forbid(unsafe_code)]

use crate::{ArrayErrorV1, DenseDouble2d, ErrorCodeV1, ErrorContextV1, LimitsV1};
use std::cell::Cell;

#[derive(Debug)]
enum ExpectedCell {
    Bits(u64),
    Nan,
}

#[derive(Debug)]
enum Expected {
    Value {
        shape: [u64; 2],
        data: Vec<ExpectedCell>,
    },
    Error {
        value: ArrayErrorV1,
        context_label: &'static str,
        code_label: &'static str,
    },
}

fn bits(data: &[f64]) -> Vec<u64> {
    data.iter().map(|value| value.to_bits()).collect()
}

fn assert_expected(result: &Result<DenseDouble2d, ArrayErrorV1>, expected: &Expected) {
    match (result, expected) {
        (Ok(value), Expected::Value { shape, data }) => {
            assert_eq!(value.shape(), *shape);
            assert_eq!(value.as_slice().len(), data.len());
            for (index, (actual, target)) in value.as_slice().iter().zip(data).enumerate() {
                match target {
                    ExpectedCell::Bits(target) => {
                        assert_eq!(actual.to_bits(), *target, "cell {index}")
                    }
                    ExpectedCell::Nan => assert!(actual.is_nan(), "cell {index}: {actual:?}"),
                }
            }
        }
        (
            Err(actual),
            Expected::Error {
                value,
                context_label,
                code_label,
            },
        ) => {
            assert_eq!(actual, value);
            assert_eq!(actual.context.as_str(), *context_label);
            assert_eq!(actual.code.as_str(), *code_label);
        }
        _ => panic!("unexpected outcome: {result:?}; expected {expected:?}"),
    }
}

fn assert_nonaliasing(value: &DenseDouble2d, inputs: &[&[f64]]) {
    if value.as_slice().is_empty() {
        return;
    }
    for input in inputs {
        if !input.is_empty() {
            assert_ne!(value.as_slice().as_ptr(), input.as_ptr());
        }
    }
}

fn check_constructor(
    shape: [u64; 2],
    source: Vec<f64>,
    limits: LimitsV1,
    expected: Expected,
    reject_reservation: bool,
    no_allocation: bool,
) {
    let original = bits(&source);
    let attempts = Cell::new(0);
    let result = if reject_reservation {
        DenseDouble2d::try_from_col_major_rejecting_reservation(shape, &source, limits, &attempts)
    } else {
        DenseDouble2d::try_from_col_major(shape, &source, limits)
    };
    assert_expected(&result, &expected);
    assert_eq!(bits(&source), original);
    if reject_reservation {
        assert_eq!(attempts.get(), 1);
    }
    if let Ok(value) = &result {
        assert_nonaliasing(value, &[&source]);
    }
    if no_allocation {
        let count = Cell::new(0);
        let probe =
            DenseDouble2d::try_from_col_major_rejecting_reservation(shape, &source, limits, &count);
        assert_eq!(
            count.get(),
            0,
            "preflight/empty result must precede reservation"
        );
        assert_expected(&probe, &expected);
        assert_eq!(bits(&source), original);
    }
    drop(source);
    assert_expected(&result, &expected);
}

fn fixture(name: &str) -> DenseDouble2d {
    let (shape, data) = match name {
        "A" => (
            [2_u64, 3_u64],
            vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64, 5.0_f64, 6.0_f64],
        ),
        "B" => (
            [2_u64, 3_u64],
            vec![10.0_f64, 20.0_f64, 30.0_f64, 40.0_f64, 50.0_f64, 60.0_f64],
        ),
        "ROW" => ([1_u64, 3_u64], vec![10.0_f64, 20.0_f64, 30.0_f64]),
        "COL" => ([2_u64, 1_u64], vec![100.0_f64, 200.0_f64]),
        "SEVEN" => ([1_u64, 1_u64], vec![7.0_f64]),
        "OTHER_SHAPE" => (
            [3_u64, 2_u64],
            vec![10.0_f64, 20.0_f64, 30.0_f64, 40.0_f64, 50.0_f64, 60.0_f64],
        ),
        "EMPTY_03" => ([0_u64, 3_u64], vec![]),
        "EMPTY_02" => ([0_u64, 2_u64], vec![]),
        "EMPTY_30" => ([3_u64, 0_u64], vec![]),
        "EMPTY_01" => ([0_u64, 1_u64], vec![]),
        "EMPTY_10" => ([1_u64, 0_u64], vec![]),
        "EMPTY_00" => ([0_u64, 0_u64], vec![]),
        "COL3" => ([3_u64, 1_u64], vec![2.0_f64, 4.0_f64, 6.0_f64]),
        "ROW4" => ([1_u64, 4_u64], vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64]),
        "COL4" => ([4_u64, 1_u64], vec![10.0_f64, 20.0_f64, 30.0_f64, 40.0_f64]),
        "POS_ZERO" => ([1_u64, 1_u64], vec![f64::from_bits(0x0000000000000000_u64)]),
        "NEG_ZERO" => ([1_u64, 1_u64], vec![f64::from_bits(0x8000000000000000_u64)]),
        "ONE" => ([1_u64, 1_u64], vec![1.0_f64]),
        "NEG_ONE" => ([1_u64, 1_u64], vec![-1.0_f64]),
        "POS_INF" => ([1_u64, 1_u64], vec![f64::from_bits(0x7ff0000000000000_u64)]),
        "NEG_INF" => ([1_u64, 1_u64], vec![f64::from_bits(0xfff0000000000000_u64)]),
        "QNAN" => ([1_u64, 1_u64], vec![f64::from_bits(0x7ff8000000000001_u64)]),
        "MAX_FINITE" => ([1_u64, 1_u64], vec![f64::from_bits(0x7fefffffffffffff_u64)]),
        "MIN_SUBNORMAL" => ([1_u64, 1_u64], vec![f64::from_bits(0x0000000000000001_u64)]),
        "TWO53" => ([1_u64, 1_u64], vec![9007199254740992.0_f64]),
        "THREE" => ([1_u64, 1_u64], vec![3.0_f64]),
        "EMPTY_20" => ([2_u64, 0_u64], vec![]),
        _ => panic!("unknown original fixture {name}"),
    };
    let original = bits(&data);
    let result = DenseDouble2d::try_from_col_major(shape, &data, LimitsV1::default())
        .expect("valid small fixture");
    assert_eq!(bits(&data), original);
    assert_eq!(bits(result.as_slice()), original);
    assert_eq!(result.shape(), shape);
    assert_nonaliasing(&result, &[&data]);
    result
}

fn check_add(
    lhs: &str,
    rhs: &str,
    limits: LimitsV1,
    expected: Expected,
    reject_reservation: bool,
    no_allocation: bool,
) {
    let lhs = fixture(lhs);
    let rhs = fixture(rhs);
    let left_shape = lhs.shape();
    let right_shape = rhs.shape();
    let left_bits = bits(lhs.as_slice());
    let right_bits = bits(rhs.as_slice());
    let attempts = Cell::new(0);
    let result = if reject_reservation {
        lhs.try_add_rejecting_reservation(&rhs, limits, &attempts)
    } else {
        lhs.try_add(&rhs, limits)
    };
    assert_expected(&result, &expected);
    assert_eq!(lhs.shape(), left_shape);
    assert_eq!(rhs.shape(), right_shape);
    assert_eq!(bits(lhs.as_slice()), left_bits);
    assert_eq!(bits(rhs.as_slice()), right_bits);
    if reject_reservation {
        assert_eq!(attempts.get(), 1);
    }
    if let Ok(value) = &result {
        assert_nonaliasing(value, &[lhs.as_slice(), rhs.as_slice()]);
    }
    if no_allocation {
        let count = Cell::new(0);
        let probe = lhs.try_add_rejecting_reservation(&rhs, limits, &count);
        assert_eq!(
            count.get(),
            0,
            "preflight/empty result must precede reservation"
        );
        assert_expected(&probe, &expected);
        assert_eq!(lhs.shape(), left_shape);
        assert_eq!(rhs.shape(), right_shape);
        assert_eq!(bits(lhs.as_slice()), left_bits);
        assert_eq!(bits(rhs.as_slice()), right_bits);
    }
    drop(lhs);
    drop(rhs);
    assert_expected(&result, &expected);
}

#[test]
fn add01() {
    check_add(
        "A",
        "B",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((22.0_f64).to_bits()),
                ExpectedCell::Bits((33.0_f64).to_bits()),
                ExpectedCell::Bits((44.0_f64).to_bits()),
                ExpectedCell::Bits((55.0_f64).to_bits()),
                ExpectedCell::Bits((66.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add01_reverse() {
    check_add(
        "B",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((22.0_f64).to_bits()),
                ExpectedCell::Bits((33.0_f64).to_bits()),
                ExpectedCell::Bits((44.0_f64).to_bits()),
                ExpectedCell::Bits((55.0_f64).to_bits()),
                ExpectedCell::Bits((66.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add02() {
    check_add(
        "A",
        "SEVEN",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((8.0_f64).to_bits()),
                ExpectedCell::Bits((9.0_f64).to_bits()),
                ExpectedCell::Bits((10.0_f64).to_bits()),
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((12.0_f64).to_bits()),
                ExpectedCell::Bits((13.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add02_reverse() {
    check_add(
        "SEVEN",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((8.0_f64).to_bits()),
                ExpectedCell::Bits((9.0_f64).to_bits()),
                ExpectedCell::Bits((10.0_f64).to_bits()),
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((12.0_f64).to_bits()),
                ExpectedCell::Bits((13.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add03() {
    check_add(
        "A",
        "ROW",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((12.0_f64).to_bits()),
                ExpectedCell::Bits((23.0_f64).to_bits()),
                ExpectedCell::Bits((24.0_f64).to_bits()),
                ExpectedCell::Bits((35.0_f64).to_bits()),
                ExpectedCell::Bits((36.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add03_reverse() {
    check_add(
        "ROW",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((11.0_f64).to_bits()),
                ExpectedCell::Bits((12.0_f64).to_bits()),
                ExpectedCell::Bits((23.0_f64).to_bits()),
                ExpectedCell::Bits((24.0_f64).to_bits()),
                ExpectedCell::Bits((35.0_f64).to_bits()),
                ExpectedCell::Bits((36.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add04() {
    check_add(
        "A",
        "COL",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((101.0_f64).to_bits()),
                ExpectedCell::Bits((202.0_f64).to_bits()),
                ExpectedCell::Bits((103.0_f64).to_bits()),
                ExpectedCell::Bits((204.0_f64).to_bits()),
                ExpectedCell::Bits((105.0_f64).to_bits()),
                ExpectedCell::Bits((206.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add04_reverse() {
    check_add(
        "COL",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((101.0_f64).to_bits()),
                ExpectedCell::Bits((202.0_f64).to_bits()),
                ExpectedCell::Bits((103.0_f64).to_bits()),
                ExpectedCell::Bits((204.0_f64).to_bits()),
                ExpectedCell::Bits((105.0_f64).to_bits()),
                ExpectedCell::Bits((206.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add05() {
    check_add(
        "ROW",
        "COL",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((110.0_f64).to_bits()),
                ExpectedCell::Bits((210.0_f64).to_bits()),
                ExpectedCell::Bits((120.0_f64).to_bits()),
                ExpectedCell::Bits((220.0_f64).to_bits()),
                ExpectedCell::Bits((130.0_f64).to_bits()),
                ExpectedCell::Bits((230.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn add05_reverse() {
    check_add(
        "COL",
        "ROW",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((110.0_f64).to_bits()),
                ExpectedCell::Bits((210.0_f64).to_bits()),
                ExpectedCell::Bits((120.0_f64).to_bits()),
                ExpectedCell::Bits((220.0_f64).to_bits()),
                ExpectedCell::Bits((130.0_f64).to_bits()),
                ExpectedCell::Bits((230.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn zero01() {
    check_add(
        "EMPTY_03",
        "ROW",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero01_reverse() {
    check_add(
        "ROW",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero02() {
    check_add(
        "EMPTY_30",
        "COL3",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [3_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero02_reverse() {
    check_add(
        "COL3",
        "EMPTY_30",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [3_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero03() {
    check_add(
        "EMPTY_01",
        "ROW",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero03_reverse() {
    check_add(
        "ROW",
        "EMPTY_01",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero04() {
    check_add(
        "EMPTY_10",
        "COL3",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [3_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero04_reverse() {
    check_add(
        "COL3",
        "EMPTY_10",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [3_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero05() {
    check_add(
        "EMPTY_01",
        "EMPTY_10",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero05_reverse() {
    check_add(
        "EMPTY_10",
        "EMPTY_01",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero06() {
    check_add(
        "EMPTY_03",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero06_reverse() {
    check_add(
        "EMPTY_03",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero07() {
    check_add(
        "EMPTY_10",
        "EMPTY_10",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero07_reverse() {
    check_add(
        "EMPTY_10",
        "EMPTY_10",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero08() {
    check_add(
        "EMPTY_00",
        "ONE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero08_reverse() {
    check_add(
        "ONE",
        "EMPTY_00",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero09() {
    check_add(
        "EMPTY_03",
        "ROW",
        LimitsV1 {
            max_dimension: 3_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn zero09_reverse() {
    check_add(
        "ROW",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 3_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn shape01() {
    check_add(
        "A",
        "OTHER_SHAPE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2_u64,
                    rhs_extent: 3_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape01_reverse() {
    check_add(
        "OTHER_SHAPE",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 3_u64,
                    rhs_extent: 2_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape02() {
    check_add(
        "EMPTY_03",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 0_u64,
                    rhs_extent: 2_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape02_reverse() {
    check_add(
        "A",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2_u64,
                    rhs_extent: 0_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape03() {
    check_add(
        "EMPTY_03",
        "EMPTY_02",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 1,
                    lhs_extent: 3_u64,
                    rhs_extent: 2_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape03_reverse() {
    check_add(
        "EMPTY_02",
        "EMPTY_03",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 1,
                    lhs_extent: 2_u64,
                    rhs_extent: 3_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape04() {
    check_add(
        "EMPTY_10",
        "ROW",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 1,
                    lhs_extent: 0_u64,
                    rhs_extent: 3_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape04_reverse() {
    check_add(
        "ROW",
        "EMPTY_10",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 1,
                    lhs_extent: 3_u64,
                    rhs_extent: 0_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn float01() {
    check_add(
        "POS_ZERO",
        "NEG_ZERO",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float01_reverse() {
    check_add(
        "NEG_ZERO",
        "POS_ZERO",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float02() {
    check_add(
        "NEG_ZERO",
        "NEG_ZERO",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x8000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float02_reverse() {
    check_add(
        "NEG_ZERO",
        "NEG_ZERO",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x8000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float03() {
    check_add(
        "ONE",
        "NEG_ONE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float03_reverse() {
    check_add(
        "NEG_ONE",
        "ONE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float04() {
    check_add(
        "POS_INF",
        "NEG_INF",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Nan],
        },
        false,
        false,
    );
}

#[test]
fn float04_reverse() {
    check_add(
        "NEG_INF",
        "POS_INF",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Nan],
        },
        false,
        false,
    );
}

#[test]
fn float05() {
    check_add(
        "POS_INF",
        "SEVEN",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x7ff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float05_reverse() {
    check_add(
        "SEVEN",
        "POS_INF",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x7ff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float06() {
    check_add(
        "NEG_INF",
        "NEG_INF",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0xfff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float06_reverse() {
    check_add(
        "NEG_INF",
        "NEG_INF",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0xfff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float07() {
    check_add(
        "QNAN",
        "SEVEN",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Nan],
        },
        false,
        false,
    );
}

#[test]
fn float07_reverse() {
    check_add(
        "SEVEN",
        "QNAN",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Nan],
        },
        false,
        false,
    );
}

#[test]
fn float08() {
    check_add(
        "MAX_FINITE",
        "MAX_FINITE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x7ff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float08_reverse() {
    check_add(
        "MAX_FINITE",
        "MAX_FINITE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x7ff0000000000000_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float09() {
    check_add(
        "MIN_SUBNORMAL",
        "MIN_SUBNORMAL",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000002_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float09_reverse() {
    check_add(
        "MIN_SUBNORMAL",
        "MIN_SUBNORMAL",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits(
                (f64::from_bits(0x0000000000000002_u64)).to_bits(),
            )],
        },
        false,
        false,
    );
}

#[test]
fn float10() {
    check_add(
        "TWO53",
        "ONE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits((9007199254740992.0_f64).to_bits())],
        },
        false,
        false,
    );
}

#[test]
fn float10_reverse() {
    check_add(
        "ONE",
        "TWO53",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits((9007199254740992.0_f64).to_bits())],
        },
        false,
        false,
    );
}

#[test]
fn float11() {
    check_add(
        "TWO53",
        "THREE",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits((9007199254740996.0_f64).to_bits())],
        },
        false,
        false,
    );
}

#[test]
fn float11_reverse() {
    check_add(
        "THREE",
        "TWO53",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1_u64, 1_u64],
            data: vec![ExpectedCell::Bits((9007199254740996.0_f64).to_bits())],
        },
        false,
        false,
    );
}

#[test]
fn ctor01() {
    check_constructor(
        [2_u64, 3_u64],
        vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64, 5.0_f64, 6.0_f64],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 3_u64],
            data: vec![
                ExpectedCell::Bits((1.0_f64).to_bits()),
                ExpectedCell::Bits((2.0_f64).to_bits()),
                ExpectedCell::Bits((3.0_f64).to_bits()),
                ExpectedCell::Bits((4.0_f64).to_bits()),
                ExpectedCell::Bits((5.0_f64).to_bits()),
                ExpectedCell::Bits((6.0_f64).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn ctor02() {
    check_constructor(
        [0_u64, 3_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [0_u64, 3_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn ctor03() {
    check_constructor(
        [0_u64, 0_u64],
        vec![],
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Value {
            shape: [0_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn ctor04() {
    check_constructor(
        [0_u64, 1_u64],
        vec![],
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 1,
                    actual: 1_u64,
                    limit: 0_u64,
                },
            },
            context_label: "construct",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn ctor05() {
    check_constructor(
        [2_u64, 3_u64],
        vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64, 5.0_f64],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::StorageLengthMismatch {
                    expected_elements: 6_u64,
                    actual_elements: 5_u64,
                },
            },
            context_label: "construct",
            code_label: "StorageLengthMismatch",
        },
        false,
        true,
    );
}

#[test]
fn ctor06() {
    check_constructor(
        [0_u64, 3_u64],
        vec![1.0_f64],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::StorageLengthMismatch {
                    expected_elements: 0_u64,
                    actual_elements: 1_u64,
                },
            },
            context_label: "construct",
            code_label: "StorageLengthMismatch",
        },
        false,
        true,
    );
}

#[test]
fn ctor07() {
    check_constructor(
        [18446744073709551615_u64, 2_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::ShapeProductOverflow {
                    rows: 18446744073709551615_u64,
                    columns: 2_u64,
                },
            },
            context_label: "construct",
            code_label: "ShapeProductOverflow",
        },
        false,
        true,
    );
}

#[test]
fn ctor08() {
    check_constructor(
        [2305843009213693952_u64, 1_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::ByteSizeOverflow {
                    elements: 2305843009213693952_u64,
                    element_width_bytes: 8_u64,
                },
            },
            context_label: "construct",
            code_label: "ByteSizeOverflow",
        },
        false,
        true,
    );
}

#[test]
fn ctor09() {
    check_constructor(
        [1048577_u64, 0_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 1048577_u64,
                    limit: 1048576_u64,
                },
            },
            context_label: "construct",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn ctor10() {
    check_constructor(
        [1025_u64, 1024_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::ElementLimitExceeded {
                    actual: 1049600_u64,
                    limit: 1048576_u64,
                },
            },
            context_label: "construct",
            code_label: "ElementLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn ctor11() {
    check_constructor(
        [5_u64, 1_u64],
        vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64, 5.0_f64],
        LimitsV1 {
            max_dimension: 100_u64,
            max_elements: 100_u64,
            max_bytes: 39_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::ByteLimitExceeded {
                    actual: 40_u64,
                    limit: 39_u64,
                },
            },
            context_label: "construct",
            code_label: "ByteLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn ctor12() {
    check_constructor(
        [1152921504606846976_u64, 1_u64],
        vec![],
        LimitsV1 {
            max_dimension: 18446744073709551615_u64,
            max_elements: 18446744073709551615_u64,
            max_bytes: 18446744073709551615_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::AddressSpaceExceeded {
                    elements: 1152921504606846976_u64,
                    payload_bytes: 9223372036854775808_u64,
                    pointer_width_bits: 64,
                },
            },
            context_label: "construct",
            code_label: "AddressSpaceExceeded",
        },
        false,
        true,
    );
}

#[test]
fn ctor13() {
    check_constructor(
        [1048576_u64, 0_u64],
        vec![],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [1048576_u64, 0_u64],
            data: vec![],
        },
        false,
        true,
    );
}

#[test]
fn limit01() {
    check_add(
        "COL4",
        "ROW4",
        LimitsV1 {
            max_dimension: 4_u64,
            max_elements: 12_u64,
            max_bytes: 128_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::ElementLimitExceeded {
                    actual: 16_u64,
                    limit: 12_u64,
                },
            },
            context_label: "add.output",
            code_label: "ElementLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn limit01_reverse() {
    check_add(
        "ROW4",
        "COL4",
        LimitsV1 {
            max_dimension: 4_u64,
            max_elements: 12_u64,
            max_bytes: 128_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::ElementLimitExceeded {
                    actual: 16_u64,
                    limit: 12_u64,
                },
            },
            context_label: "add.output",
            code_label: "ElementLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn alloc01() {
    check_constructor(
        [2_u64, 3_u64],
        vec![1.0_f64, 2.0_f64, 3.0_f64, 4.0_f64, 5.0_f64, 6.0_f64],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::Construct,
                code: ErrorCodeV1::AllocationFailure {
                    requested_elements: 6_u64,
                    requested_bytes: 48_u64,
                },
            },
            context_label: "construct",
            code_label: "AllocationFailure",
        },
        true,
        false,
    );
}

#[test]
fn alloc02() {
    check_add(
        "A",
        "B",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::AllocationFailure {
                    requested_elements: 6_u64,
                    requested_bytes: 48_u64,
                },
            },
            context_label: "add.output",
            code_label: "AllocationFailure",
        },
        true,
        false,
    );
}

#[test]
fn alloc02_reverse() {
    check_add(
        "B",
        "A",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::AllocationFailure {
                    requested_elements: 6_u64,
                    requested_bytes: 48_u64,
                },
            },
            context_label: "add.output",
            code_label: "AllocationFailure",
        },
        true,
        false,
    );
}

#[test]
fn ctor14() {
    check_constructor(
        [2_u64, 2_u64],
        vec![
            f64::from_bits(0x8000000000000000_u64),
            f64::from_bits(0x7ff8000000000001_u64),
            f64::from_bits(0x7ff0000000000000_u64),
            f64::from_bits(0x0000000000000001_u64),
        ],
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Value {
            shape: [2_u64, 2_u64],
            data: vec![
                ExpectedCell::Bits((f64::from_bits(0x8000000000000000_u64)).to_bits()),
                ExpectedCell::Bits((f64::from_bits(0x7ff8000000000001_u64)).to_bits()),
                ExpectedCell::Bits((f64::from_bits(0x7ff0000000000000_u64)).to_bits()),
                ExpectedCell::Bits((f64::from_bits(0x0000000000000001_u64)).to_bits()),
            ],
        },
        false,
        false,
    );
}

#[test]
fn shape05() {
    check_add(
        "EMPTY_20",
        "EMPTY_30",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2_u64,
                    rhs_extent: 3_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape05_reverse() {
    check_add(
        "EMPTY_30",
        "EMPTY_20",
        LimitsV1 {
            max_dimension: 1048576_u64,
            max_elements: 1048576_u64,
            max_bytes: 8388608_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 3_u64,
                    rhs_extent: 2_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape06() {
    check_add(
        "A",
        "OTHER_SHAPE",
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 2_u64,
                    rhs_extent: 3_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

#[test]
fn shape06_reverse() {
    check_add(
        "OTHER_SHAPE",
        "A",
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddCompatibility,
                code: ErrorCodeV1::IncompatibleShape {
                    axis: 0,
                    lhs_extent: 3_u64,
                    rhs_extent: 2_u64,
                },
            },
            context_label: "add.compatibility",
            code_label: "IncompatibleShape",
        },
        false,
        true,
    );
}

// Independent edge checks for explicit invariants beyond the 48 contract families.
#[test]
fn default_limits_are_the_declared_policy() {
    assert_eq!(
        LimitsV1::default(),
        LimitsV1 {
            max_dimension: 1_048_576,
            max_elements: 1_048_576,
            max_bytes: 8_388_608,
        }
    );
}

#[test]
fn huge_empty_axes_return_without_reservation_or_dimension_iteration() {
    let unbounded = LimitsV1 {
        max_dimension: u64::MAX,
        max_elements: u64::MAX,
        max_bytes: u64::MAX,
    };
    let empty_budget = LimitsV1 {
        max_dimension: u64::MAX,
        max_elements: 0,
        max_bytes: 0,
    };
    let one = fixture("ONE");
    for shape in [[0, u64::MAX], [u64::MAX, 0]] {
        let constructors = Cell::new(0);
        let empty = DenseDouble2d::try_from_col_major_rejecting_reservation(
            shape,
            &[],
            unbounded,
            &constructors,
        )
        .unwrap();
        assert_eq!(constructors.get(), 0);
        assert_eq!(empty.shape(), shape);
        for reverse in [false, true] {
            let additions = Cell::new(0);
            let result = if reverse {
                one.try_add_rejecting_reservation(&empty, empty_budget, &additions)
            } else {
                empty.try_add_rejecting_reservation(&one, empty_budget, &additions)
            }
            .unwrap();
            assert_eq!(additions.get(), 0);
            assert_eq!(result.shape(), shape);
            assert!(result.as_slice().is_empty());
            assert_eq!(empty.shape(), shape);
            assert!(empty.as_slice().is_empty());
            assert_eq!(bits(one.as_slice()), vec![1.0_f64.to_bits()]);
        }
    }
}

#[test]
fn dimension_errors_precede_element_byte_and_storage_errors() {
    let policy = LimitsV1 {
        max_dimension: 4,
        max_elements: 1,
        max_bytes: 0,
    };
    for (shape, axis) in [([5, 5], 0), ([1, 5], 1)] {
        check_constructor(
            shape,
            vec![],
            policy,
            Expected::Error {
                value: ArrayErrorV1 {
                    context: ErrorContextV1::Construct,
                    code: ErrorCodeV1::DimensionLimitExceeded {
                        axis,
                        actual: 5,
                        limit: 4,
                    },
                },
                context_label: "construct",
                code_label: "DimensionLimitExceeded",
            },
            false,
            true,
        );
    }
}

#[test]
fn construction_copies_negative_and_signaling_nan_bits_without_arithmetic() {
    let patterns = [
        0xfff8_0000_0000_0042_u64,
        0x7ff0_0000_0000_0042,
        0x8000_0000_0000_0001,
        0xfff0_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0000,
    ];
    check_constructor(
        [2, 3],
        patterns.iter().map(|x| f64::from_bits(*x)).collect(),
        LimitsV1::default(),
        Expected::Value {
            shape: [2, 3],
            data: patterns.iter().copied().map(ExpectedCell::Bits).collect(),
        },
        false,
        false,
    );
}

#[test]
fn output_dimension_cap_has_add_output_context() {
    check_add(
        "COL4",
        "ROW4",
        LimitsV1 {
            max_dimension: 3,
            max_elements: 100,
            max_bytes: 800,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 4,
                    limit: 3,
                },
            },
            context_label: "add.output",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn output_byte_cap_has_add_output_context() {
    check_add(
        "A",
        "SEVEN",
        LimitsV1 {
            max_dimension: 100,
            max_elements: 100,
            max_bytes: 39,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::ByteLimitExceeded {
                    actual: 48,
                    limit: 39,
                },
            },
            context_label: "add.output",
            code_label: "ByteLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn empty_output_still_obeys_dimension_caps() {
    check_add(
        "EMPTY_03",
        "ROW",
        LimitsV1 {
            max_dimension: 0,
            max_elements: 0,
            max_bytes: 0,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 1,
                    actual: 3,
                    limit: 0,
                },
            },
            context_label: "add.output",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn limit02() {
    check_add(
        "A",
        "B",
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 2_u64,
                    limit: 0_u64,
                },
            },
            context_label: "add.output",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn limit02_reverse() {
    check_add(
        "B",
        "A",
        LimitsV1 {
            max_dimension: 0_u64,
            max_elements: 0_u64,
            max_bytes: 0_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::DimensionLimitExceeded {
                    axis: 0,
                    actual: 2_u64,
                    limit: 0_u64,
                },
            },
            context_label: "add.output",
            code_label: "DimensionLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn limit03() {
    check_add(
        "A",
        "B",
        LimitsV1 {
            max_dimension: 100_u64,
            max_elements: 100_u64,
            max_bytes: 39_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::ByteLimitExceeded {
                    actual: 48_u64,
                    limit: 39_u64,
                },
            },
            context_label: "add.output",
            code_label: "ByteLimitExceeded",
        },
        false,
        true,
    );
}

#[test]
fn limit03_reverse() {
    check_add(
        "B",
        "A",
        LimitsV1 {
            max_dimension: 100_u64,
            max_elements: 100_u64,
            max_bytes: 39_u64,
        },
        Expected::Error {
            value: ArrayErrorV1 {
                context: ErrorContextV1::AddOutput,
                code: ErrorCodeV1::ByteLimitExceeded {
                    actual: 48_u64,
                    limit: 39_u64,
                },
            },
            context_label: "add.output",
            code_label: "ByteLimitExceeded",
        },
        false,
        true,
    );
}


#[test]
fn zeros_checked_layout_and_positive_words() {
    let limits = LimitsV1 { max_dimension: 16, max_elements: 256, max_bytes: 2048 };
    for (construct, word) in [
        (DenseDouble2d::try_zeros as fn([u64; 2], LimitsV1) -> Result<DenseDouble2d, ArrayErrorV1>, 0_u64),
        (DenseDouble2d::try_ones, 0x3ff0000000000000_u64),
    ] {
        for shape in [[1, 1], [2, 3], [16, 16]] {
            let value = construct(shape, limits).unwrap();
            assert_eq!(value.shape(), shape);
            assert_eq!(value.as_slice().len(), (shape[0] * shape[1]) as usize);
            assert!(value.as_slice().iter().all(|v| v.to_bits() == word));
        }
        assert!(matches!(construct([17, 0], limits).unwrap_err().code,
            ErrorCodeV1::DimensionLimitExceeded { axis: 0, actual: 17, limit: 16 }));
        assert!(matches!(construct([2, 3], LimitsV1 { max_elements: 5, ..limits }).unwrap_err().code,
            ErrorCodeV1::ElementLimitExceeded { actual: 6, limit: 5 }));
        assert!(matches!(construct([2, 3], LimitsV1 { max_bytes: 47, ..limits }).unwrap_err().code,
            ErrorCodeV1::ByteLimitExceeded { actual: 48, limit: 47 }));
        let wide = LimitsV1 { max_dimension: u64::MAX, max_elements: u64::MAX, max_bytes: u64::MAX };
        assert!(matches!(construct([u64::MAX, 2], wide).unwrap_err().code,
            ErrorCodeV1::ShapeProductOverflow { .. }));
        assert!(matches!(construct([u64::MAX, 1], wide).unwrap_err().code,
            ErrorCodeV1::ByteSizeOverflow { .. }));
    }
}

#[test]
fn zeros_empty_axes_validate_without_reservation() {
    let limits = LimitsV1 { max_dimension: 16, max_elements: 256, max_bytes: 2048 };
    let attempts = Cell::new(0);
    for construct in [
        DenseDouble2d::try_zeros_rejecting_reservation as fn([u64; 2], LimitsV1, &Cell<usize>) -> Result<DenseDouble2d, ArrayErrorV1>,
        DenseDouble2d::try_ones_rejecting_reservation,
        DenseDouble2d::try_eye_rejecting_reservation,
    ] {
        for shape in [[0, 0], [0, 3], [5, 0], [16, 0], [0, 16]] {
            let value = construct(shape, limits, &attempts).unwrap();
            assert_eq!(value.shape(), shape);
            assert!(value.as_slice().is_empty());
        }
        for shape in [[17, 0], [0, 17]] {
            assert!(construct(shape, limits, &attempts).is_err());
        }
    }
    let wide = LimitsV1 { max_dimension: u64::MAX, max_elements: u64::MAX, max_bytes: u64::MAX };
    for shape in [[u64::MAX,0], [0,u64::MAX]] {
        let value = DenseDouble2d::try_eye_rejecting_reservation(shape, wide, &attempts).unwrap();
        assert_eq!(value.shape(), shape);
        assert!(value.as_slice().is_empty());
    }
    assert_eq!(attempts.get(), 0);
}

#[test]
fn zeros_reservation_failure_keeps_original_allocation_cause() {
    let limits = LimitsV1 { max_dimension: 16, max_elements: 256, max_bytes: 2048 };
    for construct in [
        DenseDouble2d::try_zeros_rejecting_reservation as fn([u64; 2], LimitsV1, &Cell<usize>) -> Result<DenseDouble2d, ArrayErrorV1>,
        DenseDouble2d::try_ones_rejecting_reservation,
        DenseDouble2d::try_eye_rejecting_reservation,
    ] {
        let attempts = Cell::new(0);
        let error = construct([16, 16], limits, &attempts).unwrap_err();
        assert_eq!(attempts.get(), 1);
        assert_eq!(error, ArrayErrorV1 { context: ErrorContextV1::Construct,
            code: ErrorCodeV1::AllocationFailure { requested_elements: 256, requested_bytes: 2048 } });
    }
}


#[test]
fn eye_column_major_geometry_caps_and_overflow() {
    let limits = LimitsV1 { max_dimension: 16, max_elements: 256, max_bytes: 2048 };
    let one = 0x3ff0000000000000_u64;
    for (shape, expected) in [
        ([1,1],vec![one]), ([2,2],vec![one,0,0,one]),
        ([2,3],vec![one,0,0,one,0,0]), ([3,2],vec![one,0,0,0,one,0]),
        ([1,3],vec![one,0,0]), ([3,1],vec![one,0,0]),
    ] {
        let value = DenseDouble2d::try_eye(shape, limits).unwrap();
        assert_eq!(value.shape(), shape);
        assert_eq!(bits(value.as_slice()), expected);
    }
    let maximum = DenseDouble2d::try_eye([16,16], limits).unwrap();
    assert_eq!(maximum.as_slice().len(), 256);
    assert!(maximum.as_slice().iter().all(|v| [0,one].contains(&v.to_bits())));
    assert_eq!(maximum.as_slice().iter().enumerate().filter_map(|(i,v)| (v.to_bits()==one).then_some(i)).collect::<Vec<_>>(),
        [0_usize,17,34,51,68,85,102,119,136,153,170,187,204,221,238,255]);
    for (shape, axis) in [([17,0],0), ([0,17],1)] {
        assert_eq!(DenseDouble2d::try_eye(shape,limits).unwrap_err(), ArrayErrorV1 {
            context:ErrorContextV1::Construct, code:ErrorCodeV1::DimensionLimitExceeded {axis,actual:17,limit:16} });
    }
    assert!(matches!(DenseDouble2d::try_eye([2,3],LimitsV1 {max_elements:5,..limits}).unwrap_err().code,
        ErrorCodeV1::ElementLimitExceeded {actual:6,limit:5}));
    assert!(matches!(DenseDouble2d::try_eye([2,3],LimitsV1 {max_bytes:47,..limits}).unwrap_err().code,
        ErrorCodeV1::ByteLimitExceeded {actual:48,limit:47}));
    let wide = LimitsV1 {max_dimension:u64::MAX,max_elements:u64::MAX,max_bytes:u64::MAX};
    assert_eq!(DenseDouble2d::try_eye([u64::MAX,2],wide).unwrap_err(),ArrayErrorV1 {
        context:ErrorContextV1::Construct,code:ErrorCodeV1::ShapeProductOverflow {rows:u64::MAX,columns:2} });
}
