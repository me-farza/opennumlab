// SPDX-License-Identifier: GPL-3.0-or-later
// Original native-reference data, extracted from the accepted15-record capture.
// Provenance: operator_native_reference_v1.provenance.json
// Provenance SHA-256: e638f1859f15b111347262ba17b79631b09063eb9ee9aff711a8273274aa22a8
// F01..F06 were prediction matches. O01..O06 remain historical observations;
// this data defines only later selected Rust comparisons. No general parity.

#[derive(Debug, Clone, Copy)]
pub enum NativeExpectedV1 {
    Exact {
        class: &'static str,
        shape: [u64; 2],
        words: &'static [u64],
    },
    Nan {
        class: &'static str,
        shape: [u64; 2],
        count: usize,
    },
    ShapeError {
        native_identifier: &'static str,
        native_line: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct NativeCaseV1 {
    pub id: &'static str,
    pub source: &'static str,
    pub expected: NativeExpectedV1,
}

pub const CASES: &[NativeCaseV1] = &[
    NativeCaseV1 {
        id: "F01",
        source: "-(5-2)",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [1, 1],
            words: &[0xc008000000000000_u64],
        },
    },
    NativeCaseV1 {
        id: "F02",
        source: "[10,20,30]-[1;2]",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [2, 3],
            words: &[
                0x4022000000000000_u64,
                0x4020000000000000_u64,
                0x4033000000000000_u64,
                0x4032000000000000_u64,
                0x403d000000000000_u64,
                0x403c000000000000_u64,
            ],
        },
    },
    NativeCaseV1 {
        id: "F03",
        source: "[1,2].*[3;4]",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [2, 2],
            words: &[
                0x4008000000000000_u64,
                0x4010000000000000_u64,
                0x4018000000000000_u64,
                0x4020000000000000_u64,
            ],
        },
    },
    NativeCaseV1 {
        id: "F04",
        source: "[2;4]./[2,4,8]",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [2, 3],
            words: &[
                0x3ff0000000000000_u64,
                0x4000000000000000_u64,
                0x3fe0000000000000_u64,
                0x3ff0000000000000_u64,
                0x3fd0000000000000_u64,
                0x3fe0000000000000_u64,
            ],
        },
    },
    NativeCaseV1 {
        id: "F05",
        source: "[1,2,3;4,5,6].'",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [3, 2],
            words: &[
                0x3ff0000000000000_u64,
                0x4000000000000000_u64,
                0x4008000000000000_u64,
                0x4010000000000000_u64,
                0x4014000000000000_u64,
                0x4018000000000000_u64,
            ],
        },
    },
    NativeCaseV1 {
        id: "F06",
        source: "[]./1",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [0, 0],
            words: &[],
        },
    },
    NativeCaseV1 {
        id: "O01",
        source: "-[-0,0]",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [1, 2],
            words: &[0x0000000000000000_u64, 0x8000000000000000_u64],
        },
    },
    NativeCaseV1 {
        id: "O02",
        source: "[1,-1,1,-1]./[0,0,-0,-0]",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [1, 4],
            words: &[
                0x7ff0000000000000_u64,
                0xfff0000000000000_u64,
                0xfff0000000000000_u64,
                0x7ff0000000000000_u64,
            ],
        },
    },
    NativeCaseV1 {
        id: "O03",
        source: "[0,1e10000]./[0,1e10000]",
        expected: NativeExpectedV1::Nan {
            class: "double",
            shape: [1, 2],
            count: 2,
        },
    },
    NativeCaseV1 {
        id: "O04",
        source: "%λ\r\n[1;2]./[3;4;5]",
        expected: NativeExpectedV1::ShapeError {
            native_identifier: "MATLAB:sizeDimensionsMustMatch",
            native_line: 2,
        },
    },
    NativeCaseV1 {
        id: "O05",
        source: "1..*2",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [1, 1],
            words: &[0x4000000000000000_u64],
        },
    },
    NativeCaseV1 {
        id: "O06",
        source: "1..'",
        expected: NativeExpectedV1::Exact {
            class: "double",
            shape: [1, 1],
            words: &[0x3ff0000000000000_u64],
        },
    },
];

pub fn case(id: &str) -> &'static NativeCaseV1 {
    CASES
        .iter()
        .find(|case| case.id == id)
        .expect("identified native reference case")
}
