// SPDX-License-Identifier: GPL-3.0-or-later
//! One-shot experimental arithmetic. This route never constructs an Engine.

use anyhow::Result;
use mathlab_array::{ArrayErrorV1, ErrorCodeV1, MtimesErrorV1, MtimesWorkBudgetV1};
use mathlab_language::arithmetic_expression::{
    evaluate_arithmetic_source_v1, ArithmeticExpressionErrorV1,
};
use mathlab_language::decimal_expression::{DecimalDiagnosticDetailV1, DecimalExpressionErrorV1};
use mathlab_language::expression::{
    LiteralAdmissionFailureV1, LiteralDiagnosticDetailV1, MatrixAdmissionFailureV1,
};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::Arc;

// 16^3 covers every nonempty product admitted by arithmetic-v1. This is a
// per-operation term bound, not an execution deadline or cancellation service.
const MULTIPLICATION_BUDGET: MtimesWorkBudgetV1 = MtimesWorkBudgetV1 { max_terms: 4096 };

pub(super) fn run(source: &str, as_json: bool) -> Result<()> {
    match evaluate_arithmetic_source_v1(Arc::from(source), MULTIPLICATION_BUDGET) {
        Ok(value) => {
            let array = value.array();
            let [rows, columns] = array.shape();
            let output = if as_json {
                let words: Vec<_> = array
                    .as_slice()
                    .iter()
                    .map(|number| format!("{:016x}", number.to_bits()))
                    .collect();
                let mut record = envelope("success");
                record["value"] = json!({"class": value.class(), "shape": [rows, columns],
                    "storage_order": "column-major", "binary64_words": words});
                serde_json::to_string(&record)? + "\n"
            } else {
                let mut text = String::from("ans =\n");
                if rows == 0 || columns == 0 {
                    text.push_str(&format!("[] ({rows}x{columns})\n"));
                } else {
                    for row in 0..rows as usize {
                        for column in 0..columns as usize {
                            if column != 0 {
                                text.push(' ');
                            }
                            text.push_str(
                                &array.as_slice()[column * rows as usize + row].to_string(),
                            );
                        }
                        text.push('\n');
                    }
                }
                text
            };
            io::stdout().lock().write_all(output.as_bytes())?;
            Ok(())
        }
        Err(error) => {
            if as_json {
                let mut record = envelope("error");
                record["error"] = error_record(&error);
                writeln!(io::stdout().lock(), "{}", serde_json::to_string(&record)?)?;
            }
            let stage = format!("Rust {:?} failure", error.stage());
            Err(anyhow::Error::new(error).context(stage))
        }
    }
}

fn envelope(outcome: &str) -> Value {
    json!({"schema_version": 1, "kind": "mathlab-rust-expression", "backend": "rust",
        "profile": "arithmetic-v1", "outcome": outcome})
}

fn error_record(error: &ArithmeticExpressionErrorV1) -> Value {
    let span = error.span();
    let cause = match error {
        ArithmeticExpressionErrorV1::Syntax {
            expected, found, ..
        } => json!({"kind": "Syntax", "expected": format!("{expected:?}"),
                "found": found.map(|token| format!("{token:?}"))}),
        ArithmeticExpressionErrorV1::Multiplication { error, .. } => match error {
            MtimesErrorV1::IncompatibleInnerDimensions {
                lhs_columns,
                rhs_rows,
            } => {
                json!({"kind": "IncompatibleInnerDimensions", "lhs_columns": lhs_columns, "rhs_rows": rhs_rows})
            }
            MtimesErrorV1::WorkCountOverflow {
                output_elements,
                inner_extent,
            } => {
                json!({"kind": "WorkCountOverflow", "output_elements": output_elements, "inner_extent": inner_extent})
            }
            MtimesErrorV1::WorkLimitExceeded {
                required_terms,
                max_terms,
            } => {
                json!({"kind": "WorkLimitExceeded", "required_terms": required_terms, "max_terms": max_terms})
            }
            MtimesErrorV1::Array(error) => array_error(error),
        },
        ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Allocation {
            requested_items,
            cause,
            ..
        }) => {
            json!({"kind": "Allocation", "requested_items": requested_items, "message": cause.to_string()})
        }
        ArithmeticExpressionErrorV1::Shared(DecimalExpressionErrorV1::Diagnostic(error)) => {
            match &error.detail {
                DecimalDiagnosticDetailV1::Profile(detail) => profile_detail(detail),
                DecimalDiagnosticDetailV1::Syntax { expected, found } => {
                    json!({"kind": "Syntax", "expected": format!("{expected:?}"),
                    "found": found.map(|token| format!("{token:?}"))})
                }
                DecimalDiagnosticDetailV1::Malformed(reason) => {
                    json!({"kind": "MalformedDecimal", "reason": format!("{reason:?}")})
                }
                DecimalDiagnosticDetailV1::ConversionRejected(error) => {
                    json!({"kind": "ConversionRejected", "message": error.to_string()})
                }
            }
        }
    };
    json!({"code": error.code(), "stage": format!("{:?}", error.stage()),
        "span": {"start": span.start, "end": span.end,
            "start_line": span.start_line, "end_line": span.end_line},
        "message": error.to_string(), "cause": cause})
}

fn profile_detail(detail: &LiteralDiagnosticDetailV1) -> Value {
    match detail {
        LiteralDiagnosticDetailV1::SourceForm(form) => {
            json!({"kind": "SourceForm", "form": format!("{form:?}")})
        }
        LiteralDiagnosticDetailV1::Identifier => json!({"kind": "Identifier"}),
        LiteralDiagnosticDetailV1::Character(character) => {
            json!({"kind": "Character", "character": character})
        }
        LiteralDiagnosticDetailV1::Syntax { expected, found } => {
            json!({"kind": "Syntax", "expected": format!("{expected:?}"),
                "found": found.map(|token| format!("{token:?}"))})
        }
        LiteralDiagnosticDetailV1::Resource {
            name,
            limit,
            attempted_count,
        } => {
            json!({"kind": "Resource", "name": name.as_str(), "limit": limit, "attempted_count": attempted_count})
        }
        LiteralDiagnosticDetailV1::Literal(reason) => match reason {
            LiteralAdmissionFailureV1::TooManyDigits { actual, limit } => {
                json!({"kind": "TooManyDigits", "actual": actual, "limit": limit})
            }
            LiteralAdmissionFailureV1::IntegerOverflow => json!({"kind": "IntegerOverflow"}),
            LiteralAdmissionFailureV1::Magnitude { actual, limit } => {
                json!({"kind": "Magnitude", "actual": actual, "limit": limit})
            }
        },
        LiteralDiagnosticDetailV1::Matrix(reason) => match reason {
            MatrixAdmissionFailureV1::RaggedRows { row_lengths } => {
                json!({"kind": "RaggedRows", "row_lengths": row_lengths})
            }
            MatrixAdmissionFailureV1::Dimension {
                axis,
                actual,
                limit,
            } => json!({"kind": "Dimension", "axis": axis, "actual": actual, "limit": limit}),
            MatrixAdmissionFailureV1::Elements { actual, limit } => {
                json!({"kind": "Elements", "actual": actual, "limit": limit})
            }
            MatrixAdmissionFailureV1::Bytes { actual, limit } => {
                json!({"kind": "Bytes", "actual": actual, "limit": limit})
            }
        },
        LiteralDiagnosticDetailV1::ArrayOperation { error } => array_error(error),
    }
}

fn array_error(error: &ArrayErrorV1) -> Value {
    let mut record = match error.code {
        ErrorCodeV1::ShapeProductOverflow { rows, columns } => {
            json!({"rows": rows, "columns": columns})
        }
        ErrorCodeV1::ByteSizeOverflow {
            elements,
            element_width_bytes,
        } => json!({"elements": elements, "element_width_bytes": element_width_bytes}),
        ErrorCodeV1::DimensionLimitExceeded {
            axis,
            actual,
            limit,
        } => json!({"axis": axis, "actual": actual, "limit": limit}),
        ErrorCodeV1::ElementLimitExceeded { actual, limit }
        | ErrorCodeV1::ByteLimitExceeded { actual, limit } => {
            json!({"actual": actual, "limit": limit})
        }
        ErrorCodeV1::AddressSpaceExceeded {
            elements,
            payload_bytes,
            pointer_width_bits,
        } => {
            json!({"elements": elements, "payload_bytes": payload_bytes, "pointer_width_bits": pointer_width_bits})
        }
        ErrorCodeV1::StorageLengthMismatch {
            expected_elements,
            actual_elements,
        } => json!({"expected_elements": expected_elements, "actual_elements": actual_elements}),
        ErrorCodeV1::IncompatibleShape {
            axis,
            lhs_extent,
            rhs_extent,
        } => json!({"axis": axis, "lhs_extent": lhs_extent, "rhs_extent": rhs_extent}),
        ErrorCodeV1::AllocationFailure {
            requested_elements,
            requested_bytes,
        } => json!({"requested_elements": requested_elements, "requested_bytes": requested_bytes}),
    };
    record["kind"] = json!(error.code.as_str());
    record["context"] = json!(error.context.as_str());
    record
}
