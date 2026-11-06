// SPDX-License-Identifier: GPL-3.0-or-later
//! Original cooperative column-pivoted Householder least squares. No normal equations.
//! Factors are private candidates; the caller owns result publication and reserve rollback.
use super::{
    Complex64V1 as C, ScientificClassV1, ScientificControlV1, ScientificDenseV1 as Dense,
    ScientificErrorV1 as E, ScientificShapeV1 as Shape, reserve,
};

// Diagnostic-only observer. Default cfg strips this API and every call below.
#[cfg(onl_qr_phase_probe)]
thread_local!{static QR_PHASE_PROBE:std::cell::Cell<Option<fn(ScientificQrPhaseV1)>>=const{std::cell::Cell::new(None)};static QR_PHASE_PROBE_ACTIVE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[cfg(onl_qr_phase_probe)]
pub struct ScientificQrPhaseProbeGuardV1{_thread_affine:std::marker::PhantomData<std::rc::Rc<()>>}
#[cfg(onl_qr_phase_probe)]
pub fn install_scientific_qr_phase_probe_v1(callback:fn(ScientificQrPhaseV1))->ScientificQrPhaseProbeGuardV1{
 QR_PHASE_PROBE_ACTIVE.with(|active|assert!(!active.replace(true),"diagnostic QR observer reentry"));
 QR_PHASE_PROBE.with(|slot|slot.set(Some(callback)));
 ScientificQrPhaseProbeGuardV1{_thread_affine:std::marker::PhantomData}
}
#[cfg(onl_qr_phase_probe)]
impl Drop for ScientificQrPhaseProbeGuardV1{fn drop(&mut self){QR_PHASE_PROBE.with(|slot|slot.set(None));QR_PHASE_PROBE_ACTIVE.with(|active|active.set(false));}}
#[cfg(onl_qr_phase_probe)]
fn observe_first_qr_apply_b(phase:ScientificQrPhaseV1){QR_PHASE_PROBE.with(|slot|{if let Some(callback)=slot.take(){callback(phase)}});}

#[derive(Debug, Clone, Copy, Default)]
struct Norm {
    scale: f64,
    scaled: f64,
}
impl Norm {
    fn value(self) -> Result<f64, E> {
        finite(self.scale * self.scaled)
    }
}
#[derive(Debug, Clone, Copy, Default)]
struct Head {
    leading: C,
    phase: C,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScientificQrPhaseV1 {
    Prepared,
    Norms,
    Swap,
    Reflector,
    ApplyA,
    ApplyB,
    Phase,
    Pack,
    Backsolve,
    Complete,
    Failed,
}
#[derive(Debug, Clone, Copy)]
pub struct ScientificQrDiagnosticsV1 {
    pub rows: usize,
    pub columns: usize,
    pub right_hand_sides: usize,
    pub scalar_units: u64,
    pub rank_ratio_threshold: f64,
    pub minimum_scaled_pivot: f64,
}
#[derive(Debug)]
pub struct ScientificQrV1 {
    qr: Vec<C>,
    y: Vec<C>,
    u: Vec<C>,
    heads: Vec<Head>,
    permutation: Vec<u32>,
    norms: Vec<Norm>,
    x: Dense,
    m: usize,
    n: usize,
    k: usize,
    work: u64,
    phase: ScientificQrPhaseV1,
    max_column_norm: f64,
    minimum_scaled_pivot: f64,
}
struct Count<'a, T> {
    inner: &'a mut T,
    units: &'a mut u64,
}
impl<T: ScientificControlV1> ScientificControlV1 for Count<'_, T> {
    fn charge(&mut self, n: u64) -> Result<(), E> {
        self.inner.charge(n)?;
        *self.units = self.units.checked_add(n).ok_or(E::Resource)?;
        Ok(())
    }
    fn reserve(&mut self, n: usize) -> Result<(), E> {
        self.inner.reserve(n)
    }
    async fn checkpoint(&mut self, n: u64) -> Result<(), E> {
        self.inner.checkpoint(n).await?;
        *self.units = self.units.checked_add(n).ok_or(E::Resource)?;
        Ok(())
    }
}
fn finite(x: f64) -> Result<f64, E> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(E::ArithmeticBreakdown)
    }
}
fn zfinite(z: C) -> Result<C, E> {
    if z.finite() {
        Ok(z)
    } else {
        Err(E::ArithmeticBreakdown)
    }
}
fn div_real(x: f64, d: f64) -> Result<f64, E> {
    if !d.is_finite() || d <= 0. {
        return Err(E::ArithmeticBreakdown);
    }
    let y = finite(x / d)?;
    Ok(y)
}
fn scale_div(z: C, d: f64) -> Result<C, E> {
    Ok(C {
        re: div_real(z.re, d)?,
        im: div_real(z.im, d)?,
    })
}
fn product(a: f64, b: f64) -> Result<f64, E> {
    let p = finite(a * b)?;
    Ok(p)
}
fn multiply(a: C, b: C) -> Result<C, E> {
    Ok(C {
        re: finite(product(a.re, b.re)? - product(a.im, b.im)?)?,
        im: finite(product(a.re, b.im)? + product(a.im, b.re)?)?,
    })
}
fn add(a: C, b: C) -> Result<C, E> {
    zfinite(a.add(b))
}
fn subtract(a: C, b: C) -> Result<C, E> {
    zfinite(a.sub(b))
}
fn phase(z: C) -> Result<C, E> {
    let s = z.re.abs().max(z.im.abs());
    if s == 0. {
        return Ok(C::real(1.));
    }
    let z = scale_div(z, s)?;
    let d = finite((z.re * z.re + z.im * z.im).sqrt())?;
    scale_div(z, d)
}
async fn norm(values: &[C], control: &mut impl ScientificControlV1) -> Result<Norm, E> {
    let mut scale = 0.0f64;
    for chunk in values.chunks(8) {
        control.checkpoint((chunk.len() * 16) as u64).await?;
        for z in chunk {
            if !z.finite() {
                return Err(E::ArithmeticBreakdown);
            }
            scale = scale.max(z.re.abs()).max(z.im.abs());
        }
    }
    if scale == 0. {
        return Ok(Norm::default());
    }
    let mut sum = 0.0f64;
    for chunk in values.chunks(8) {
        control.checkpoint((chunk.len() * 16) as u64).await?;
        for &z in chunk {
            let q = scale_div(z, scale)?;
            sum = finite(sum + q.re * q.re + q.im * q.im)?;
        }
    }
    control.checkpoint(128).await?;
    let scaled = finite(sum.sqrt())?;
    let out = Norm { scale, scaled };
    out.value()?;
    Ok(out)
}
async fn filled<T: Copy>(
    count: usize,
    value: T,
    control: &mut impl ScientificControlV1,
) -> Result<Vec<T>, E> {
    let mut out = reserve(count, control)?;
    for chunk in (0..count).step_by(16) {
        let end = (chunk + 16).min(count);
        control.checkpoint(((end - chunk) * 8) as u64).await?;
        for _ in chunk..end {
            out.push(value)
        }
    }
    Ok(out)
}
async fn copied(value: &Dense, control: &mut impl ScientificControlV1) -> Result<Vec<C>, E> {
    let mut out = reserve(value.len(), control)?;
    for first in (0..value.len()).step_by(16) {
        let end = (first + 16).min(value.len());
        control.checkpoint(((end - first) * 8) as u64).await?;
        for i in first..end {
            out.push(value.get(i).ok_or(E::Index)?);
        }
    }
    Ok(out)
}
impl ScientificQrV1 {
    fn shape(a: &Dense, b: &Dense) -> Result<(usize, usize, usize), E> {
        if a.class() != ScientificClassV1::Double || b.class() != ScientificClassV1::Double {
            return Err(E::Type);
        }
        let aa = a.shape();
        let bb = b.shape();
        if aa.rank() != 2 || bb.rank() != 2 {
            return Err(E::Rank);
        }
        let m = aa.axes()[0] as usize;
        let n = aa.axes()[1] as usize;
        let k = bb.axes()[1] as usize;
        if m <= n || n == 0 || k == 0 || bb.axes()[0] as usize != m {
            return Err(E::Shape);
        }
        Ok((m, n, k))
    }
    /// Private candidates/scratch, excluding retained caller inputs and fixed future/stack metadata.
    pub fn candidate_bytes(a: &Dense, b: &Dense) -> Result<usize, E> {
        let (m, n, k) = Self::shape(a, b)?;
        let unit = if a.is_complex() || b.is_complex() {
            16
        } else {
            8
        };
        let parts = [
            m.checked_mul(n).and_then(|v| v.checked_mul(16)),
            m.checked_mul(k).and_then(|v| v.checked_mul(16)),
            m.checked_mul(16),
            n.checked_mul(32),
            n.checked_mul(4),
            n.checked_mul(16),
            n.checked_mul(k).and_then(|v| v.checked_mul(unit)),
        ];
        parts.into_iter().try_fold(0usize, |sum, x| {
            sum.checked_add(x.ok_or(E::Resource)?).ok_or(E::Resource)
        })
    }
    pub async fn new_cooperative(
        a: &Dense,
        b: &Dense,
        available_bytes: usize,
        control: &mut impl ScientificControlV1,
    ) -> Result<Self, E> {
        let (m, n, k) = Self::shape(a, b)?;
        if Self::candidate_bytes(a, b)? > available_bytes.min(64 << 20) {
            return Err(E::Resource);
        }
        let mut work = 0;
        let mut count = Count {
            inner: control,
            units: &mut work,
        };
        for value in [a, b] {
            for first in (0..value.len()).step_by(16) {
                let end = (first + 16).min(value.len());
                count.checkpoint(((end - first) * 8) as u64).await?;
                for i in first..end {
                    if !value.get(i).ok_or(E::Index)?.finite() {
                        return Err(E::NonFiniteInput);
                    }
                }
            }
        }
        let qr = copied(a, &mut count).await?;
        let y = copied(b, &mut count).await?;
        let u = filled(m, C::ZERO, &mut count).await?;
        let heads = filled(n, Head::default(), &mut count).await?;
        let mut permutation = filled(n, 0u32, &mut count).await?;
        for first in (0..n).step_by(16) {
            let end = (first + 16).min(n);
            count.checkpoint(((end - first) * 4) as u64).await?;
            for i in first..end {
                permutation[i] = u32::try_from(i).map_err(|_| E::Resource)?;
            }
        }
        let norms = filled(n, Norm::default(), &mut count).await?;
        let x = Dense::zeros_cooperative(
            Shape::new(&[n as u64, k as u64])?,
            a.is_complex() || b.is_complex(),
            &mut count,
        )
        .await?;
        drop(count);
        let out = Self {
            qr,
            y,
            u,
            heads,
            permutation,
            norms,
            x,
            m,
            n,
            k,
            work,
            phase: ScientificQrPhaseV1::Prepared,
            max_column_norm: 0.,
            minimum_scaled_pivot: f64::INFINITY,
        };
        if out.capacity_bytes() != Self::candidate_bytes(a, b)? {
            return Err(E::Resource);
        }
        Ok(out)
    }
    pub fn capacity_bytes(&self) -> usize {
        self.qr.capacity() * 16
            + self.y.capacity() * 16
            + self.u.capacity() * 16
            + self.heads.capacity() * std::mem::size_of::<Head>()
            + self.permutation.capacity() * 4
            + self.norms.capacity() * std::mem::size_of::<Norm>()
            + self.x.capacity_bytes()
    }
    pub fn phase(&self) -> ScientificQrPhaseV1 {
        self.phase
    }
    pub fn diagnostics(&self) -> ScientificQrDiagnosticsV1 {
        ScientificQrDiagnosticsV1 {
            rows: self.m,
            columns: self.n,
            right_hand_sides: self.k,
            scalar_units: self.work,
            rank_ratio_threshold: 64. * self.m.max(self.n) as f64 * f64::EPSILON / 2.,
            minimum_scaled_pivot: self.minimum_scaled_pivot,
        }
    }
    /// A dropped or failed solve cannot be resumed through this API: candidates stay private until drop.
    pub async fn solve_cooperative(
        &mut self,
        control: &mut impl ScientificControlV1,
    ) -> Result<(), E> {
        if self.phase != ScientificQrPhaseV1::Prepared {
            return Err(E::Resource);
        }
        self.phase = ScientificQrPhaseV1::Failed;
        let mut work = self.work;
        let result = {
            let mut count = Count {
                inner: control,
                units: &mut work,
            };
            self.solve_inner(&mut count).await
        };
        self.work = work;
        if result.is_ok() {
            self.phase = ScientificQrPhaseV1::Complete
        } else {
            self.phase = ScientificQrPhaseV1::Failed
        }
        result
    }
    async fn solve_inner(&mut self, control: &mut impl ScientificControlV1) -> Result<(), E> {
        let threshold = 64. * self.m.max(self.n) as f64 * f64::EPSILON / 2.;
        for p in 0..self.n {
            self.phase = ScientificQrPhaseV1::Norms;
            for j in p..self.n {
                self.norms[j] = norm(&self.qr[p + j * self.m..(j + 1) * self.m], control).await?;
            }
            let mut best = p;
            let mut best_value = self.norms[p].value()?;
            for j in p..self.n {
                control.checkpoint(8).await?;
                let value = self.norms[j].value()?;
                if p == 0 {
                    self.max_column_norm = self.max_column_norm.max(value)
                }
                if value > best_value
                    || (value == best_value && self.permutation[j] < self.permutation[best])
                {
                    best = j;
                    best_value = value;
                }
            }
            if self.max_column_norm == 0. || best_value / self.max_column_norm <= threshold {
                return Err(E::Singular);
            }
            self.minimum_scaled_pivot = self
                .minimum_scaled_pivot
                .min(best_value / self.max_column_norm);
            self.phase = ScientificQrPhaseV1::Swap;
            if best != p {
                for first in (0..self.m).step_by(8) {
                    let end = (first + 8).min(self.m);
                    control.checkpoint(((end - first) * 16) as u64).await?;
                    for row in first..end {
                        self.qr.swap(row + p * self.m, row + best * self.m);
                    }
                }
                control.checkpoint(8).await?;
                self.permutation.swap(p, best);
                self.norms.swap(p, best);
            }
            self.phase = ScientificQrPhaseV1::Reflector;
            control.checkpoint(128).await?;
            let scale = self.norms[p].scale;
            let nrm = self.norms[p].value()?;
            let phi = phase(self.qr[p + p * self.m])?;
            for first in (p..self.m).step_by(4) {
                let end = (first + 4).min(self.m);
                control.checkpoint(((end - first) * 32) as u64).await?;
                for row in first..end {
                    self.u[row] = scale_div(self.qr[row + p * self.m], scale)?;
                }
            }
            control.checkpoint(128).await?;
            self.u[p] = add(
                self.u[p],
                C {
                    re: product(phi.re, self.norms[p].scaled)?,
                    im: product(phi.im, self.norms[p].scaled)?,
                },
            )?;
            let unorm = norm(&self.u[p..], control).await?.value()?;
            for first in (p..self.m).step_by(4) {
                let end = (first + 4).min(self.m);
                control.checkpoint(((end - first) * 32) as u64).await?;
                for row in first..end {
                    self.u[row] = scale_div(self.u[row], unorm)?;
                }
            }
            self.phase = ScientificQrPhaseV1::ApplyA;
            for j in p + 1..self.n {
                let mut dot = C::ZERO;
                for first in (p..self.m).step_by(2) {
                    let end = (first + 2).min(self.m);
                    control.checkpoint(((end - first) * 64) as u64).await?;
                    for row in first..end {
                        dot = add(
                            dot,
                            multiply(self.u[row].conj(), self.qr[row + j * self.m])?,
                        )?;
                    }
                }
                for first in (p..self.m).step_by(2) {
                    let end = (first + 2).min(self.m);
                    control.checkpoint(((end - first) * 64) as u64).await?;
                    for row in first..end {
                        let delta = multiply(self.u[row], dot)?;
                        let delta = C {
                            re: product(2., delta.re)?,
                            im: product(2., delta.im)?,
                        };
                        self.qr[row + j * self.m] = subtract(self.qr[row + j * self.m], delta)?;
                    }
                }
            }
            self.phase = ScientificQrPhaseV1::ApplyB;
            #[cfg(onl_qr_phase_probe)]
            if p==0{observe_first_qr_apply_b(self.phase);}
            for j in 0..self.k {
                let mut dot = C::ZERO;
                for first in (p..self.m).step_by(2) {
                    let end = (first + 2).min(self.m);
                    control.checkpoint(((end - first) * 64) as u64).await?;
                    for row in first..end {
                        dot = add(dot, multiply(self.u[row].conj(), self.y[row + j * self.m])?)?;
                    }
                }
                for first in (p..self.m).step_by(2) {
                    let end = (first + 2).min(self.m);
                    control.checkpoint(((end - first) * 64) as u64).await?;
                    for row in first..end {
                        let delta = multiply(self.u[row], dot)?;
                        let delta = C {
                            re: product(2., delta.re)?,
                            im: product(2., delta.im)?,
                        };
                        self.y[row + j * self.m] = subtract(self.y[row + j * self.m], delta)?;
                    }
                }
            }
            self.phase = ScientificQrPhaseV1::Phase;
            let rotation = C {
                re: -phi.re,
                im: phi.im,
            };
            for j in p + 1..self.n {
                control.checkpoint(32).await?;
                self.qr[p + j * self.m] = multiply(rotation, self.qr[p + j * self.m])?;
            }
            for j in 0..self.k {
                control.checkpoint(32).await?;
                self.y[p + j * self.m] = multiply(rotation, self.y[p + j * self.m])?;
            }
            control.checkpoint(32).await?;
            self.qr[p + p * self.m] = C::real(nrm);
            self.heads[p] = Head {
                leading: self.u[p],
                phase: rotation,
            };
            self.phase = ScientificQrPhaseV1::Pack;
            for first in (p + 1..self.m).step_by(16) {
                let end = (first + 16).min(self.m);
                control.checkpoint(((end - first) * 8) as u64).await?;
                for row in first..end {
                    self.qr[row + p * self.m] = self.u[row];
                }
            }
        }
        self.phase = ScientificQrPhaseV1::Backsolve;
        for rhs in 0..self.k {
            for row in (0..self.n).rev() {
                let mut value = self.y[row + rhs * self.m];
                for col in row + 1..self.n {
                    control.checkpoint(64).await?;
                    let x = self
                        .x
                        .get(self.permutation[col] as usize + rhs * self.n)
                        .ok_or(E::Index)?;
                    value = subtract(value, multiply(self.qr[row + col * self.m], x)?)?;
                }
                control.checkpoint(128).await?;
                let x = scale_div(value, self.qr[row + row * self.m].re)?;
                self.x
                    .set(self.permutation[row] as usize + rhs * self.n, x)?;
            }
        }
        Ok(())
    }
    pub fn into_result(self) -> Result<(Dense, ScientificQrDiagnosticsV1), E> {
        if self.phase != ScientificQrPhaseV1::Complete {
            return Err(E::Resource);
        }
        let diagnostics = self.diagnostics();
        Ok((self.x, diagnostics))
    }
}

#[cfg(test)]
mod factor_tests {
    use super::*;
    #[derive(Default)]
    struct Control;
    impl ScientificControlV1 for Control {
        fn charge(&mut self, n: u64) -> Result<(), E> {
            assert!(n <= 128);
            Ok(())
        }
        fn reserve(&mut self, _: usize) -> Result<(), E> {
            Ok(())
        }
    }
    #[test]
    fn packed_complex_reflectors_reconstruct_permuted_input_and_unitary_thin_q() {
        let words = [
            C { re: 1., im: 1. },
            C::ZERO,
            C { re: 1., im: -1. },
            C { re: 2., im: 1. },
            C { re: 3., im: -1. },
            C { re: 1., im: 2. },
            C { re: 0., im: 1. },
            C { re: -1., im: 1. },
        ];
        let a = Dense::from_complex(Shape::new(&[4, 2]).unwrap(), &words, &mut Control).unwrap();
        let b = Dense::from_complex(
            Shape::new(&[4, 1]).unwrap(),
            &[C::ONE, C::ONE, C::ONE, C::ONE],
            &mut Control,
        )
        .unwrap();
        let mut job = super::super::drive(ScientificQrV1::new_cooperative(
            &a,
            &b,
            64 << 20,
            &mut Control,
        ))
        .unwrap();
        super::super::drive(job.solve_cooperative(&mut Control)).unwrap();
        assert_eq!(
            job.permutation[0], 1,
            "stronger second column must pivot first"
        );
        let mut q = vec![C::ZERO; job.m * job.n];
        for j in 0..job.n {
            q[j + j * job.m] = C::ONE;
        }
        // Independent application of the stored mathematical H*D^H factors to identity.
        for p in (0..job.n).rev() {
            for col in 0..job.n {
                q[p + col * job.m] = job.heads[p].phase.conj().mul(q[p + col * job.m]);
                let mut dot = C::ZERO;
                for r in p..job.m {
                    let u = if r == p {
                        job.heads[p].leading
                    } else {
                        job.qr[r + p * job.m]
                    };
                    dot = dot.add(u.conj().mul(q[r + col * job.m]));
                }
                for r in p..job.m {
                    let u = if r == p {
                        job.heads[p].leading
                    } else {
                        job.qr[r + p * job.m]
                    };
                    q[r + col * job.m] = q[r + col * job.m].sub(u.mul(dot).mul(C::real(2.)));
                }
            }
        }
        for i in 0..job.n {
            for j in 0..job.n {
                let mut value = C::ZERO;
                for r in 0..job.m {
                    value = value.add(q[r + i * job.m].conj().mul(q[r + j * job.m]));
                }
                let expected = if i == j { 1. } else { 0. };
                assert!((value.re - expected).abs() < 1e-12 && value.im.abs() < 1e-12);
            }
        }
        for j in 0..job.n {
            assert!(job.qr[j + j * job.m].re > 0.);
            assert_eq!(job.qr[j + j * job.m].im, 0.);
            for row in 0..job.m {
                let mut reconstructed = C::ZERO;
                for k in 0..=j {
                    reconstructed =
                        reconstructed.add(q[row + k * job.m].mul(job.qr[k + j * job.m]));
                }
                let expected = words[row + job.permutation[j] as usize * job.m];
                assert!(
                    (reconstructed.re - expected.re).abs() < 1e-12
                        && (reconstructed.im - expected.im).abs() < 1e-12
                );
            }
        }
    }
}
