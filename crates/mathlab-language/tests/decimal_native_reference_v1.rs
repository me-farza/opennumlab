// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected accepted MATLAB R2026b decimal source/value regressions, F01-F24.
//! Native words come directly from batch 20261016T035041036492Z records; the
//! separate decimal_native_reference_v1.provenance.json proves embedded bytes.
//! No runtime research-file dependency or general converter parity is claimed.
//! O01-O13 are observation-only inputs, not additional positive value cases.
//! Native-valid 1D2, 1.*2, 1./2 and 1.' remain outside this profile; observed
//! native identifiers are not mappings to project diagnostic codes.

use mathlab_language::decimal_expression::evaluate_decimal_source_v1;
use std::sync::Arc;

#[test]
fn twenty_four_accepted_decimal_sources_match_native_value_words() {
    // (native case, exact original source, native class, shape, column-major words)
    let cases: &[(&str, &str, &str, [u64; 2], &[u64])] = &[
        ("F01", ".5", "double", [1, 1], &[0x3fe0000000000000]),
        ("F02", "1.", "double", [1, 1], &[0x3ff0000000000000]),
        ("F03", "1.e2", "double", [1, 1], &[0x4059000000000000]),
        ("F04", ".5E+1", "double", [1, 1], &[0x4014000000000000]),
        (
            "F05",
            "001.2500e-1",
            "double",
            [1, 1],
            &[0x3fc0000000000000],
        ),
        ("F06", "1e2+3", "double", [1, 1], &[0x4059c00000000000]),
        ("F07", "- 0e+99", "double", [1, 1], &[0x8000000000000000]),
        ("F08", "+ .0", "double", [1, 1], &[0x0000000000000000]),
        (
            "F09",
            "1.00000000000000011102230246251565404236316680908203125",
            "double",
            [1, 1],
            &[0x3ff0000000000000],
        ),
        (
            "F10",
            "1.00000000000000011102230246251565404236316680908203126",
            "double",
            [1, 1],
            &[0x3ff0000000000001],
        ),
        ("F11", "5e-324", "double", [1, 1], &[0x0000000000000001]),
        (
            "F12",
            "2.2250738585072014e-308",
            "double",
            [1, 1],
            &[0x0010000000000000],
        ),
        (
            "F13",
            "1.7976931348623157e308",
            "double",
            [1, 1],
            &[0x7fefffffffffffff],
        ),
        ("F14", "1e10000", "double", [1, 1], &[0x7ff0000000000000]),
        ("F15", "-1e-10000", "double", [1, 1], &[0x8000000000000000]),
        (
            "F16",
            "[1.5,3e0;-2.25,.5]",
            "double",
            [2, 2],
            &[
                0x3ff8000000000000,
                0xc002000000000000,
                0x4008000000000000,
                0x3fe0000000000000,
            ],
        ),
        (
            "F17",
            "%λ\n( - .50e+1 + 2. ) % tail\n",
            "double",
            [1, 1],
            &[0xc008000000000000],
        ),
        ("F18", "0.1", "double", [1, 1], &[0x3fb999999999999a]),
        (
            "F19",
            "2.4703282292062327e-324",
            "double",
            [1, 1],
            &[0x0000000000000000],
        ),
        (
            "F20",
            "2.4703282292062328e-324",
            "double",
            [1, 1],
            &[0x0000000000000001],
        ),
        (
            "F21",
            "2.225073858507201e-308",
            "double",
            [1, 1],
            &[0x000fffffffffffff],
        ),
        (
            "F22",
            "1.7976931348623158e308",
            "double",
            [1, 1],
            &[0x7fefffffffffffff],
        ),
        (
            "F23",
            "1.7976931348623159e308",
            "double",
            [1, 1],
            &[0x7ff0000000000000],
        ),
        (
            "F24",
            "1.00000000000000033306690738754696212708950042724609375",
            "double",
            [1, 1],
            &[0x3ff0000000000002],
        ),
    ];
    assert_eq!(cases.len(), 24);
    for (id, original, class, shape, expected_bits) in cases {
        let source: Arc<str> = Arc::from(*original);
        let value = evaluate_decimal_source_v1(Arc::clone(&source))
            .unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert!(Arc::ptr_eq(&source, value.source()), "{id}");
        assert!(Arc::ptr_eq(&source, value.ast().source()), "{id}");
        assert_eq!(value.source().as_ref(), *original, "{id}");
        assert_eq!(value.class(), *class, "{id}");
        assert_eq!(value.array().shape(), *shape, "{id}");
        let actual_bits: Vec<u64> = value
            .array()
            .as_slice()
            .iter()
            .map(|element| element.to_bits())
            .collect();
        assert_eq!(actual_bits.as_slice(), *expected_bits, "{id}");
        let (parsed, array) = value.into_parts();
        assert!(Arc::ptr_eq(&source, parsed.source()), "{id}");
        drop(source);
        assert_eq!(parsed.source().as_ref(), *original, "{id}");
        assert_eq!(array.shape(), *shape, "{id}");
    }
}
