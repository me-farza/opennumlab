// SPDX-License-Identifier: GPL-3.0-or-later
//! Original exact-data provider tests. Not a MATLAB rank-policy parity claim.
use mathlab_array::scientific::*;
use std::future::Future;
#[derive(Default)]
struct Control {
    steps: u64,
    sites: usize,
    reserved: usize,
    max_chunk: u64,
    fail: Option<usize>,
    stop: Option<u64>,
}
impl ScientificControlV1 for Control {
    fn charge(&mut self, n: u64) -> Result<(), ScientificErrorV1> {
        assert!(n <= 128);
        self.max_chunk = self.max_chunk.max(n);
        self.steps += n;
        if self.stop.is_some_and(|at| self.steps >= at) {
            Err(ScientificErrorV1::Cancelled)
        } else {
            Ok(())
        }
    }
    fn reserve(&mut self, n: usize) -> Result<(), ScientificErrorV1> {
        self.sites += 1;
        if self.fail == Some(self.sites) {
            return Err(ScientificErrorV1::Allocation);
        }
        self.reserved += n;
        Ok(())
    }
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
}
fn real(axes: &[u64], words: &[f64]) -> ScientificDenseV1 {
    ScientificDenseV1::from_real(
        ScientificShapeV1::new(axes).unwrap(),
        words,
        &mut Control::default(),
    )
    .unwrap()
}
fn complex(axes: &[u64], pairs: &[(f64, f64)]) -> ScientificDenseV1 {
    let words: Vec<_> = pairs
        .iter()
        .map(|&(re, im)| Complex64V1 { re, im })
        .collect();
    ScientificDenseV1::from_complex(
        ScientificShapeV1::new(axes).unwrap(),
        &words,
        &mut Control::default(),
    )
    .unwrap()
}
async fn solve(
    a: &ScientificDenseV1,
    b: &ScientificDenseV1,
    control: &mut Control,
) -> Result<(ScientificDenseV1, ScientificQrDiagnosticsV1), ScientificErrorV1> {
    let mut job = ScientificQrV1::new_cooperative(a, b, 64 << 20, control).await?;
    job.solve_cooperative(control).await?;
    job.into_result()
}
fn sc06() -> (ScientificDenseV1, ScientificDenseV1) {
    (
        real(&[3, 2], &[1., 0., 1., 0., 1., 1.]),
        real(&[3, 1], &[1., 2., 4.]),
    )
}
#[test]
fn qr_reproduces_native_sc06_tall_solution_true_residual_and_seven_reserves() {
    let (a, b) = sc06();
    let mut c = Control::default();
    let (x, d) = drive(solve(&a, &b, &mut c)).unwrap();
    assert!(!x.is_complex());
    assert_eq!(x.shape().axes(), [2, 1]);
    assert!((x.get(0).unwrap().re - 4. / 3.).abs() < 1e-12);
    assert!((x.get(1).unwrap().re - 7. / 3.).abs() < 1e-12);
    for r in 0..3 {
        let value = a.get(r).unwrap().re * x.get(0).unwrap().re
            + a.get(r + 3).unwrap().re * x.get(1).unwrap().re
            - b.get(r).unwrap().re;
        assert!((value - [1. / 3., 1. / 3., -1. / 3.][r]).abs() < 1e-12);
    }
    assert_eq!(c.sites, 7);
    assert_eq!(c.reserved, ScientificQrV1::candidate_bytes(&a, &b).unwrap());
    assert_eq!(c.steps, d.scalar_units);
    assert!(c.max_chunk <= 128);
    assert!(d.minimum_scaled_pivot > d.rank_ratio_threshold);
    assert_eq!(a.real_words().unwrap(), [1., 0., 1., 0., 1., 1.]);
    assert_eq!(b.real_words().unwrap(), [1., 2., 4.]);
}
#[test]
fn qr_genuinely_dense_complex_forced_column_pivot_two_rhs_known_solution() {
    let a = complex(
        &[4, 2],
        &[
            (1., 1.),
            (0., 0.),
            (1., -1.),
            (2., 1.),
            (3., -1.),
            (1., 2.),
            (0., 1.),
            (-1., 1.),
        ],
    );
    let b = complex(
        &[4, 2],
        &[
            (5., -3.),
            (4., 3.),
            (3., 2.),
            (0., 6.),
            (-1.25, 1.25),
            (0., 1.25),
            (0.75, 3.5),
            (-4.75, 3.25),
        ],
    );
    let (x, d) = drive(solve(&a, &b, &mut Control::default())).unwrap();
    assert!(x.is_complex());
    assert_eq!((d.rows, d.columns, d.right_hand_sides), (4, 2, 2));
    for (i, (re, im)) in [(1., 1.), (2., -1.), (-1., 2.), (0.5, 0.25)]
        .into_iter()
        .enumerate()
    {
        let z = x.get(i).unwrap();
        assert!(
            (z.re - re).abs() < 1e-12 && (z.im - im).abs() < 1e-12,
            "{i}: {z:?}"
        );
    }
}
#[test]
fn qr_rank_nonfinite_shape_and_headroom_refusal_never_returns_partial_result() {
    let dependent = real(&[3, 2], &[1., 2., 3., 2., 4., 6.]);
    let b = real(&[3, 1], &[1., 2., 3.]);
    assert!(matches!(
        drive(solve(&dependent, &b, &mut Control::default())),
        Err(ScientificErrorV1::Singular)
    ));
    let bad = real(&[3, 2], &[1., 0., 1., 0., f64::NAN, 1.]);
    assert!(matches!(
        drive(solve(&bad, &b, &mut Control::default())),
        Err(ScientificErrorV1::NonFiniteInput)
    ));
    let (a, b) = sc06();
    let required = ScientificQrV1::candidate_bytes(&a, &b).unwrap();
    let mut c = Control::default();
    assert!(matches!(
        drive(ScientificQrV1::new_cooperative(
            &a,
            &b,
            required - 1,
            &mut c
        )),
        Err(ScientificErrorV1::Resource)
    ));
    assert_eq!(c.sites, 0);
    let square = real(&[2, 2], &[1., 0., 0., 1.]);
    assert_eq!(
        ScientificQrV1::candidate_bytes(&square, &real(&[2, 1], &[1., 2.])),
        Err(ScientificErrorV1::Shape)
    );
    let wide = real(&[1, 2], &[1., 1.]);
    assert_eq!(
        ScientificQrV1::candidate_bytes(&wide, &real(&[1, 1], &[2.])),
        Err(ScientificErrorV1::Shape)
    );
}
#[test]
fn qr_every_actual_reserve_failure_preserves_borrowed_inputs_and_fresh_job_recovers() {
    let (a, b) = sc06();
    for site in 1..=7 {
        let mut c = Control {
            fail: Some(site),
            ..Default::default()
        };
        assert!(matches!(
            drive(solve(&a, &b, &mut c)),
            Err(ScientificErrorV1::Allocation)
        ));
        assert_eq!(c.sites, site);
        assert_eq!(a.real_words().unwrap(), [1., 0., 1., 0., 1., 1.]);
        assert_eq!(b.real_words().unwrap(), [1., 2., 4.]);
        let (x, _) = drive(solve(&a, &b, &mut Control::default())).unwrap();
        assert!((x.get(0).unwrap().re - 4. / 3.).abs() < 1e-12);
    }
    // Meter records admissions only; persistent release/accounting is the caller's scoped-ledger gate.
}
#[inline(never)]
fn baseline_polls(a: &ScientificDenseV1, b: &ScientificDenseV1) -> (usize, u64) {
    let mut c = Control::default();
    let mut future = std::pin::pin!(solve(a, b, &mut c));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut polls = 0;
    loop {
        polls += 1;
        assert!(polls <= 4096);
        if let std::task::Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return (polls, result.unwrap().1.scalar_units);
        }
    }
}
#[inline(never)]
fn drop_at(a: &ScientificDenseV1, b: &ScientificDenseV1, at: usize) {
    let mut c = Control::default();
    let mut future = std::pin::pin!(solve(a, b, &mut c));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..at {
        assert!(future.as_mut().poll(&mut cx).is_pending());
    }
}
#[test]
fn qr_every_tiny_pending_drop_and_charged_stop_preserves_inputs_and_recovers() {
    let (a, b) = sc06();
    let (polls, units) = baseline_polls(&a, &b);
    assert!(polls > 1 && units > 0);
    for at in 0..polls {
        drop_at(&a, &b, at);
        let (x, _) = drive(solve(&a, &b, &mut Control::default())).unwrap();
        assert!((x.get(0).unwrap().re - 4. / 3.).abs() < 1e-12);
    }
    // Every reached charged checkpoint, not every integer unit of a chunk.
    let mut stops = Vec::new();
    struct Probe<'a>(&'a mut Vec<u64>, u64);
    impl ScientificControlV1 for Probe<'_> {
        fn charge(&mut self, n: u64) -> Result<(), ScientificErrorV1> {
            self.1 += n;
            self.0.push(self.1);
            Ok(())
        }
        fn reserve(&mut self, _: usize) -> Result<(), ScientificErrorV1> {
            Ok(())
        }
    }
    let mut probe = Probe(&mut stops, 0);
    let mut job = drive(ScientificQrV1::new_cooperative(
        &a,
        &b,
        64 << 20,
        &mut probe,
    ))
    .unwrap();
    drive(job.solve_cooperative(&mut probe)).unwrap();
    drop(job);
    drop(probe);
    for stop in stops {
        assert!(matches!(
            drive(solve(
                &a,
                &b,
                &mut Control {
                    stop: Some(stop),
                    ..Default::default()
                }
            )),
            Err(ScientificErrorV1::Cancelled)
        ));
        assert_eq!(b.real_words().unwrap(), [1., 2., 4.]);
        assert!(drive(solve(&a, &b, &mut Control::default())).is_ok());
    }
}

#[test]
fn qr_project_rank_boundary_is_common_scale_invariant_and_nonfinite_rhs_refuses() {
    for scale in [2f64.powi(-500), 1., 2f64.powi(500)] {
        for (epsilon, admitted) in [(2f64.powi(-30), true), (2f64.powi(-52), false)] {
            let a = real(&[3, 2], &[scale, 0., 0., 0., scale * epsilon, 0.]);
            let b = real(&[3, 1], &[scale, 2. * scale * epsilon, 0.]);
            let result = drive(solve(&a, &b, &mut Control::default()));
            if admitted {
                let (x, _) = result.unwrap();
                assert!((x.get(0).unwrap().re - 1.).abs() < 1e-12);
                assert!((x.get(1).unwrap().re - 2.).abs() < 1e-12);
            } else {
                assert!(matches!(result, Err(ScientificErrorV1::Singular)));
            }
        }
    }
    let (a, _) = sc06();
    let b = real(&[3, 1], &[1., f64::INFINITY, 4.]);
    let mut c = Control::default();
    assert!(matches!(
        drive(solve(&a, &b, &mut c)),
        Err(ScientificErrorV1::NonFiniteInput)
    ));
    assert_eq!(c.sites, 0);
    let tiny = f64::from_bits(1);
    let a = real(&[3, 2], &[tiny, 0., 0., 0., tiny, 0.]);
    let b = real(&[3, 1], &[1., 1., 0.]);
    assert!(matches!(
        drive(solve(&a, &b, &mut Control::default())),
        Err(ScientificErrorV1::ArithmeticBreakdown)
    ));
}

#[test]
fn qr_dense_complex_inconsistent_multiple_rhs_matches_exact_gram_stationarity_certificate() {
    // Exact dyadic data/certificate in tiny-noisy-complex-01.json; A^H A has offdiag2-i,det148.
    let a = complex(
        &[4, 2],
        &[
            (1., 1.),
            (0., 0.),
            (1., -1.),
            (2., 1.),
            (3., -1.),
            (1., 2.),
            (0., 1.),
            (0., 1.),
        ],
    );
    let b = complex(
        &[4, 2],
        &[
            (5., -8.),
            (-5., 5.),
            (8., 2.),
            (2., 5.),
            (1.25, 2.5),
            (1.25, -3.75),
            (-0.5, 6.),
            (-4.25, 3.5),
        ],
    );
    let (x, _) = drive(solve(&a, &b, &mut Control::default())).unwrap();
    for (i, (re, im)) in [(1., 1.), (2., -1.), (-1., 2.), (0.5, 0.25)]
        .into_iter()
        .enumerate()
    {
        let z = x.get(i).unwrap();
        assert!((z.re - re).abs() <= 2f64.powi(-33) && (z.im - im).abs() <= 2f64.powi(-33));
    }
    let mut norm2 = 0.;
    for rhs in 0..2 {
        let mut residual = [Complex64V1::ZERO; 4];
        for row in 0..4 {
            let mut y = Complex64V1::ZERO;
            for col in 0..2 {
                y = y.add(
                    a.get(row + 4 * col)
                        .unwrap()
                        .mul(x.get(col + 2 * rhs).unwrap()),
                );
            }
            residual[row] = y.sub(b.get(row + 4 * rhs).unwrap());
            norm2 += residual[row].re * residual[row].re + residual[row].im * residual[row].im;
        }
        for col in 0..2 {
            let mut normal = Complex64V1::ZERO;
            for row in 0..4 {
                normal = normal.add(a.get(row + 4 * col).unwrap().conj().mul(residual[row]));
            }
            assert!(normal.re.abs() <= 2f64.powi(-30) && normal.im.abs() <= 2f64.powi(-30));
        }
    }
    assert!(
        norm2 > 1.,
        "genuinely inconsistent, not only a manufactured consistent solve"
    );
}

#[test]
fn qr_uniform_subnormal_sc06_scale_keeps_moderate_representable_solution() {
    let scale = f64::from_bits(1u64 << 51); // Exactly2^-1023, not a zero/normal-only admission rule.
    let a = real(&[3, 2], &[scale, 0., scale, 0., scale, scale]);
    let b = real(&[3, 1], &[scale, 2. * scale, 4. * scale]);
    let (x, _) = drive(solve(&a, &b, &mut Control::default())).unwrap();
    assert!((x.get(0).unwrap().re - 4. / 3.).abs() < 1e-12);
    assert!((x.get(1).unwrap().re - 7. / 3.).abs() < 1e-12);
    assert_eq!(a.real_words().unwrap()[0].to_bits(), scale.to_bits());
}

#[test]
fn qr_huge_orthogonal_rhs_with_subnormal_well_conditioned_a_has_zero_solution() {
    // Known Source02 range gate: common A/B scaling must not overflow a zero-Q-weight row.
    let s = f64::from_bits(1u64 << 51);
    let huge = f64::from_bits(2046u64 << 52);
    let a = real(&[3, 2], &[s, 0., 0., 0., s, 0.]);
    let b = real(&[3, 1], &[0., 0., huge]);
    let (x, _) = drive(solve(&a, &b, &mut Control::default())).unwrap();
    assert_eq!(x.get(0).unwrap().re, 0.);
    assert_eq!(x.get(1).unwrap().re, 0.);
    assert_eq!(b.real_words().unwrap()[2].to_bits(), huge.to_bits());
}
