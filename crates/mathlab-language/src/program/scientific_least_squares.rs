// SPDX-License-Identifier: GPL-3.0-or-later
//! Additive tall-only bridge. Existing V1/V2 numerical enums and square LU are unchanged.
use super::{ScientificControlV1, ScientificErrorV1 as E, ScientificProgramValueV2 as V};
pub use mathlab_array::scientific::{
    ScientificQrDiagnosticsV1, ScientificQrPhaseV1, ScientificQrV1,
};
pub fn scientific_least_squares_bytes_v1(a: &V, b: &V) -> Result<usize, E> {
    ScientificQrV1::candidate_bytes(a.dense_view()?, b.dense_view()?)
}
pub async fn evaluate_scientific_least_squares_v1(
    a: &V,
    b: &V,
    available_bytes: usize,
    control: &mut impl ScientificControlV1,
) -> Result<(V, ScientificQrDiagnosticsV1), E> {
    let mut job =
        ScientificQrV1::new_cooperative(a.dense_view()?, b.dense_view()?, available_bytes, control)
            .await?;
    job.solve_cooperative(control).await?;
    let (value, diagnostics) = job.into_result()?;
    Ok((V::from_dense(value), diagnostics))
}
