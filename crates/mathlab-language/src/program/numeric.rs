// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    value::{LIMITS, Storage, layout},
    *,
};
use mathlab_array::MtimesWorkBudgetV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramNumericOperationV1 {
    Unary(UnaryOperatorV1),
    Binary(BinaryOperatorV1),
    Transpose,
    Sum,
    Mean,
    Prod,
}
fn broadcast(a: [u64; 2], b: [u64; 2]) -> Result<[u64; 2], ProgramValueErrorV1> {
    let mut out = [0; 2];
    for i in 0..2 {
        out[i] = if a[i] == b[i] {
            a[i]
        } else if a[i] == 1 {
            b[i]
        } else if b[i] == 1 {
            a[i]
        } else {
            return Err(ProgramValueErrorV1::Shape);
        }
    }
    layout(out)?;
    Ok(out)
}
fn logical(op: ProgramNumericOperationV1) -> bool {
    matches!(
        op,
        ProgramNumericOperationV1::Unary(UnaryOperatorV1::Not)
            | ProgramNumericOperationV1::Binary(
                BinaryOperatorV1::Less
                    | BinaryOperatorV1::LessEqual
                    | BinaryOperatorV1::Greater
                    | BinaryOperatorV1::GreaterEqual
                    | BinaryOperatorV1::Equal
                    | BinaryOperatorV1::NotEqual
                    | BinaryOperatorV1::And
                    | BinaryOperatorV1::Or
                    | BinaryOperatorV1::ShortCircuitAnd
                    | BinaryOperatorV1::ShortCircuitOr
            )
    )
}
pub fn program_numeric_output_layout_v1(
    op: ProgramNumericOperationV1,
    a: &ProgramValueV1,
    b: Option<&ProgramValueV1>,
) -> Result<([u64; 2], ProgramValueClassV1), ProgramValueErrorV1> {
    use BinaryOperatorV1 as B;
    use ProgramNumericOperationV1 as O;
    let s = a.shape();
    let shape = match op {
        O::Transpose => [s[1], s[0]],
        O::Sum | O::Mean | O::Prod => [1, if s[0] == 1 || s == [0, 0] { 1 } else { s[1] }],
        O::Binary(B::Multiply) => {
            let t = b.ok_or(ProgramValueErrorV1::Type)?.shape();
            if s == [1, 1] {
                t
            } else if t == [1, 1] {
                s
            } else if s[1] == t[0] {
                [s[0], t[1]]
            } else {
                return Err(ProgramValueErrorV1::Shape);
            }
        }
        O::Binary(_) => broadcast(s, b.ok_or(ProgramValueErrorV1::Type)?.shape())?,
        _ => s,
    };
    layout(shape)?;
    Ok((
        shape,
        if logical(op) {
            ProgramValueClassV1::Logical
        } else {
            a.class()
        },
    ))
}
pub fn evaluate_program_numeric_controlled_v1(
    op: ProgramNumericOperationV1,
    a: &ProgramValueV1,
    b: Option<&ProgramValueV1>,
    mut poll: impl FnMut() -> bool,
) -> Result<ProgramValueV1, ProgramValueErrorV1> {
    use BinaryOperatorV1 as B;
    use ProgramNumericOperationV1 as O;
    if poll() {
        return Err(ProgramValueErrorV1::Cancelled);
    }
    if matches!(op, O::Binary(B::ShortCircuitAnd | B::ShortCircuitOr))
        && (a.shape() != [1, 1] || b.is_none_or(|v| v.shape() != [1, 1]))
    {
        return Err(ProgramValueErrorV1::Shape);
    }
    let (shape, class) = program_numeric_output_layout_v1(op, a, b)?;
    let output = if let Storage::Double(x) = &a.storage {
        let y = b.and_then(|v| match &v.storage {
            Storage::Double(a) => Some(a),
            _ => None,
        });
        let arr = match op {
            O::Unary(UnaryOperatorV1::Plus) => Some(
                mathlab_array::DenseDouble2d::try_from_col_major(x.shape(), x.as_slice(), LIMITS)?,
            ),
            O::Unary(UnaryOperatorV1::Minus) => Some(
                mathlab_array::DenseDouble2d::try_from_col_major(x.shape(), x.as_slice(), LIMITS)?
                    .into_negated(),
            ),
            O::Transpose => Some(x.try_transpose(LIMITS)?),
            O::Sum => Some(crate::session_expression::exact_reduction::sum(x)?),
            O::Mean => Some(crate::session_expression::exact_reduction::reduce(
                x, false,
            )?),
            O::Prod => Some(crate::session_expression::exact_reduction::reduce(x, true)?),
            O::Binary(B::Add) => Some(x.try_add(y.ok_or(ProgramValueErrorV1::Type)?, LIMITS)?),
            O::Binary(B::Subtract) => Some(x.try_sub(y.ok_or(ProgramValueErrorV1::Type)?, LIMITS)?),
            O::Binary(B::Multiply) => Some(
                x.try_mtimes(
                    y.ok_or(ProgramValueErrorV1::Type)?,
                    LIMITS,
                    MtimesWorkBudgetV1 { max_terms: 4096 },
                )
                .map_err(ProgramValueErrorV1::Multiply)?,
            ),
            O::Binary(B::ElementMultiply) => {
                Some(x.try_times(y.ok_or(ProgramValueErrorV1::Type)?, LIMITS)?)
            }
            O::Binary(B::ElementDivide) => {
                Some(x.try_rdivide(y.ok_or(ProgramValueErrorV1::Type)?, LIMITS)?)
            }
            O::Binary(B::ElementLeftDivide) => {
                Some(y.ok_or(ProgramValueErrorV1::Type)?.try_rdivide(x, LIMITS)?)
            }
            O::Binary(B::Divide) => {
                let y = y.ok_or(ProgramValueErrorV1::Type)?;
                if y.shape() != [1, 1] {
                    return Err(ProgramValueErrorV1::UnsupportedFeature);
                }
                Some(x.try_rdivide(y, LIMITS)?)
            }
            O::Binary(B::LeftDivide) => {
                if x.shape() != [1, 1] {
                    return Err(ProgramValueErrorV1::UnsupportedFeature);
                }
                Some(y.ok_or(ProgramValueErrorV1::Type)?.try_rdivide(x, LIMITS)?)
            }
            _ => None,
        };
        arr.map(|array| ProgramValueV1 {
            storage: Storage::Double(array),
        })
    } else {
        None
    };
    let output = if let Some(v) = output {
        v
    } else {
        let n = layout(shape)?;
        let mut doubles = [0.0; 256];
        let mut bools = [0u8; 256];
        let ash = a.shape();
        let bsh = b.map(ProgramValueV1::shape).unwrap_or([1, 1]);
        for i in 0..n {
            if i % 128 == 0 && poll() {
                return Err(ProgramValueErrorV1::Cancelled);
            }
            let row = i % shape[0] as usize;
            let col = i / shape[0] as usize;
            let offset = |s: [u64; 2]| {
                (if s[0] == 1 { 0 } else { row })
                    + (if s[1] == 1 { 0 } else { col }) * s[0] as usize
            };
            let x = a
                .word(if op == O::Transpose {
                    col + row * ash[0] as usize
                } else {
                    offset(ash)
                })
                .ok_or(ProgramValueErrorV1::Shape)?;
            let y = b.and_then(|b| b.word(offset(bsh))).unwrap_or(0.0);
            let v = match op {
                O::Unary(UnaryOperatorV1::Not) => (x == 0.0) as u8 as f64,
                O::Transpose => {
                    let j = col + row * ash[0] as usize;
                    a.word(j).ok_or(ProgramValueErrorV1::Shape)?
                }
                O::Binary(o) => match o {
                    B::Less => (x < y) as u8 as f64,
                    B::LessEqual => (x <= y) as u8 as f64,
                    B::Greater => (x > y) as u8 as f64,
                    B::GreaterEqual => (x >= y) as u8 as f64,
                    B::Equal => (x == y) as u8 as f64,
                    B::NotEqual => (x != y) as u8 as f64,
                    B::And | B::ShortCircuitAnd => (x != 0.0 && y != 0.0) as u8 as f64,
                    B::Or | B::ShortCircuitOr => (x != 0.0 || y != 0.0) as u8 as f64,
                    B::Power | B::ElementPower => {
                        if a.class() != ProgramValueClassV1::Double
                            || b.is_none_or(|v| v.class() != ProgramValueClassV1::Double)
                            || !y.is_finite()
                            || y.fract() != 0.0
                            || y.abs() > 1024.0
                            || (o == B::Power
                                && (a.shape() != [1, 1] || b.unwrap().shape() != [1, 1]))
                        {
                            return Err(ProgramValueErrorV1::UnsupportedFeature);
                        }
                        let mut base = if y < 0.0 { 1.0 / x } else { x };
                        let mut exponent = y.abs() as u32;
                        let mut result = 1.0;
                        while exponent > 0 {
                            if exponent & 1 != 0 {
                                result *= base
                            }
                            exponent >>= 1;
                            if exponent > 0 {
                                base *= base
                            }
                        }
                        result
                    }
                    _ => return Err(ProgramValueErrorV1::Type),
                },
                _ => return Err(ProgramValueErrorV1::Type),
            };
            doubles[i] = v;
            bools[i] = (v != 0.0) as u8;
        }
        if class == ProgramValueClassV1::Logical {
            ProgramValueV1::try_logical(shape, &bools[..n])?
        } else {
            ProgramValueV1::try_double(shape, &doubles[..n])?
        }
    };
    if poll() {
        return Err(ProgramValueErrorV1::Cancelled);
    }
    Ok(output)
}
