#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent expected records from constants/index math, never runtime output.

These mathematical oracles are neither MATLAB executions nor saved-reference
goldens. Updating a fixture requires separately reviewing its expected contract.
"""

import argparse
import json
import math
from pathlib import Path


def column_major(rows):
    return [rows[row][column] for column in range(len(rows[0])) for row in range(len(rows))]


def value(name, class_name, shape, real, imag=None, coordinates=None):
    if coordinates is None:
        assert len(real) == math.prod(shape)
        row_indices, column_indices = [], []
    else:
        row_indices, column_indices = coordinates
        assert len(real) == len(row_indices) == len(column_indices)
    assert imag is None or len(imag) == len(real)
    return {
        "name": name,
        "class": class_name,
        "shape": shape,
        "complex": imag is not None,
        "storage": "dense" if coordinates is None else "sparse",
        "rows": row_indices,
        "columns": column_indices,
        "real": [str(item) for item in real],
        "imag": [] if imag is None else [str(item) for item in imag],
    }


def scalar(name, number):
    return value(name, "double", [1, 1], [number])


def record(case_id, values, identifier="", message=""):
    return {
        "schema_version": 1,
        "case_id": case_id,
        "outcome": "error" if identifier or message else "success",
        "values": sorted(values, key=lambda item: item["name"]),
        "error": {"identifier": identifier, "message": message},
    }


def integer_boundaries():
    return [
        value("exact", "uint64", [1, 1], [2**53 + 17]),
        value("unsigned_limits", "uint64", [1, 2], [0, 2**64 - 1]),
        value("signed_limits", "int64", [1, 2], [-(2**63), 2**63 - 1]),
    ]


def sparse_logical(name):
    return value(name, "logical", [3, 4], [1, 1], coordinates=([1, 3], [2, 4]))


def expected_records():
    original = list(range(1, 61))
    copied = original.copy()
    # One-based (2,3,4) in a column-major [3,4,5] array.
    selected_index = (2 - 1) + 3 * (3 - 1) + 3 * 4 * (4 - 1)
    assert original[selected_index] == 44
    copied[selected_index] = 444
    exact_nd = record("exact_nd_uint64", integer_boundaries() + [
        value("original", "double", [3, 4, 5], original),
        value("copy", "double", [3, 4, 5], copied),
        value("plane", "double", [3, 4], list(range(37, 49))),
        scalar("selected_before", 44), scalar("selected_after", 444),
    ])
    complex_real = column_major([[1, -2], [0, 0.5]])
    complex_imag = column_major([[3, 0], [-4, 0.25]])
    complex_sparse = record("complex_sparse", [
        value("complex_double", "double", [2, 2], complex_real, complex_imag),
        value("complex_single", "single", [2, 2], complex_real, complex_imag),
        value("sparse_real", "double", [3, 4], [3, -4, 0.5],
              coordinates=([1, 3, 2], [1, 2, 4])),
        value("sparse_complex", "double", [3, 4], [2, -1], [3, 4],
              coordinates=([1, 3], [2, 4])),
        sparse_logical("sparse_logical"),
        value("logical", "logical", [2, 2], column_major([[1, 0], [0, 1]])),
        value("empty", "double", [0, 3], []),
    ])
    closure = record("closure_forward_local", [
        scalar("initial", 4), scalar("first", 7), scalar("second", 9),
        scalar("anonymous_capture", 12), scalar("forward", 9),
    ])
    intentional = record("intentional_error", [
        value("pre_error", "uint32", [1, 2], [7, 11]),
    ], "MathLab:ConformanceIntentional", "Original conformance failure for transport testing.")
    mat = record("mat_v7", integer_boundaries() + [
        value("cube", "uint8", [2, 3, 4], list(range(1, 25))),
        value("complex_double", "double", [2, 2],
              column_major([[1, 0], [-2, 0.5]]), column_major([[0, 3], [4, -0.25]])),
        value("single", "single", [2, 2], column_major([[0.5, -2], [4, 8]])),
        value("logical", "logical", [2, 2], column_major([[1, 0], [0, 1]])),
        value("sparse_real", "double", [3, 4], [2, -1],
              coordinates=([1, 3], [2, 4])),
        value("sparse_complex", "double", [3, 4], [2, -1], [3, 4],
              coordinates=([1, 3], [2, 4])),
        value("empty", "double", [0, 3, 2], []),
        value("nonfinite", "double", [1, 3], ["NaN", "Inf", "-Inf"]),
    ])
    known_gap = record("mat_sparse_logical", [sparse_logical("original"), sparse_logical("loaded")])
    return [exact_nd, complex_sparse, closure, intentional, mat, known_gap]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Check committed mathematical oracles without writing")
    args = parser.parse_args()
    destination = Path(__file__).resolve().parent / "oracles"
    if not args.check:
        destination.mkdir(exist_ok=True)
    for expected in expected_records():
        path = destination / (expected["case_id"] + ".json")
        encoded = json.dumps(expected, ensure_ascii=False, indent=2) + "\n"
        if args.check:
            if not path.is_file() or path.read_text() != encoded:
                raise SystemExit("Independent oracle differs: " + str(path))
        else:
            path.write_text(encoded)
    print("Six independent constant/index-math oracles " + ("checked." if args.check else "written."))


if __name__ == "__main__":
    main()
