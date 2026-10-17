// SPDX-License-Identifier: GPL-3.0-or-later
//! Original Rust test declarations transcribed from the canonical S01-S10
//! fixtures. No JSON parser or runtime reference execution is involved.
//! S01 has native-observed values/times; endpoint/status policy and all other
//! cases are project policy. These tests have not been run in this handoff.

use super::*;
use std::cell::Cell;

fn channel(id: u64, period_ticks: u64, initial_state: f64, increment: f64) -> ChannelPlanV1 {
    ChannelPlanV1 {
        id,
        period_ticks,
        initial_state,
        increment,
    }
}

fn plan(base_step: f64, last_tick: u64, channels: Vec<ChannelPlanV1>) -> PlanV1 {
    PlanV1 {
        revision: 17,
        base_step,
        last_tick,
        channels,
    }
}

fn s01_plan() -> PlanV1 {
    plan(
        f64::from_bits(0x3fb999999999999a),
        6,
        vec![channel(1, 1, 0.0, 1.0), channel(2, 2, 0.0, 10.0)],
    )
}

fn small_limits() -> LimitsV1 {
    LimitsV1 {
        max_channels: 16,
        max_base_ticks: 128,
        max_channel_visits: 2048,
        max_state_additions: 2048,
        max_trace_samples: 2048,
        max_trace_payload_bytes: 2048 * size_of::<SampleV1>() as u64,
    }
}

fn unrestricted_counts() -> LimitsV1 {
    LimitsV1 {
        max_channels: u64::MAX,
        max_base_ticks: u64::MAX,
        max_channel_visits: u64::MAX,
        max_state_additions: u64::MAX,
        max_trace_samples: u64::MAX,
        max_trace_payload_bytes: u64::MAX,
    }
}

fn finish(mut run: PreparedRunV1) -> RunResultV1 {
    // Every execution fixture here has at most seven base ticks. A bounded
    // helper makes a cursor regression fail instead of hanging the test suite.
    for _ in 0..128 {
        if run.status().is_some() {
            return run.into_result().unwrap();
        }
        run.advance(ControlV1::Continue).unwrap();
    }
    panic!("small fixture did not terminate within 128 calls");
}

fn run(plan: PlanV1) -> RunResultV1 {
    finish(PreparedRunV1::prepare(plan, small_limits()).unwrap())
}

fn assert_trace(result: &RunResultV1, index: usize, id: u64, ticks: &[u64], value_bits: &[u64]) {
    let trace = &result.traces[index];
    assert_eq!(trace.channel_id, id);
    assert_eq!(trace.samples().len(), ticks.len());
    assert_eq!(ticks.len(), value_bits.len());
    for ((sample, tick), bits) in trace.samples().iter().zip(ticks).zip(value_bits) {
        assert_eq!(sample.tick, *tick);
        assert_eq!(sample.value.to_bits(), *bits, "channel {id} at tick {tick}");
    }
}

fn assert_times_bits(result: &RunResultV1, index: usize, bits: &[u64]) {
    assert_eq!(result.traces[index].samples().len(), bits.len());
    for (sample, expected) in result.traces[index].samples().iter().zip(bits) {
        assert_eq!(sample.time.to_bits(), *expected);
    }
}

#[test]
fn s01_native_observed_two_rates_with_project_endpoint_and_status() {
    let result = run(s01_plan());
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.revision, 17);
    assert_eq!(result.profile, PROFILE_V1);
    assert_eq!(result.last_tick, 6);
    assert_eq!(result.base_step.to_bits(), 0x3fb999999999999a);
    assert_eq!(result.last_committed_tick, Some(6));
    assert_eq!(result.traces.len(), 2);
    assert_trace(
        &result,
        0,
        1,
        &[0, 1, 2, 3, 4, 5, 6],
        &[
            0x0000000000000000,
            0x3ff0000000000000,
            0x4000000000000000,
            0x4008000000000000,
            0x4010000000000000,
            0x4014000000000000,
            0x4018000000000000,
        ],
    );
    assert_trace(
        &result,
        1,
        2,
        &[0, 2, 4, 6],
        &[
            0x0000000000000000,
            0x4024000000000000,
            0x4034000000000000,
            0x403e000000000000,
        ],
    );
    // Native timestamp comparison is tolerant, as explicitly scoped in S01.
    let native_fast_times = [0.0_f64, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
    let native_slow_times = [0.0_f64, 0.2, 0.4, 0.6];
    for (index, expected) in [(0, &native_fast_times[..]), (1, &native_slow_times[..])] {
        for (sample, time) in result.traces[index].samples().iter().zip(expected) {
            assert!((sample.time - time).abs() <= 1e-12);
        }
    }
    // Original project clock policy: the sixth display time is 6 * binary64 .1.
    assert_eq!(
        result.traces[0].samples()[6].time.to_bits(),
        0x3fe3333333333334
    );
}

#[test]
fn s02_project_dyadic_discrete_samples_equal_two_t() {
    let result = run(plan(0.25, 4, vec![channel(1, 1, 0.0, 0.5)]));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.last_committed_tick, Some(4));
    assert_trace(
        &result,
        0,
        1,
        &[0, 1, 2, 3, 4],
        &[
            0x0000000000000000,
            0x3fe0000000000000,
            0x3ff0000000000000,
            0x3ff8000000000000,
            0x4000000000000000,
        ],
    );
    assert_times_bits(
        &result,
        0,
        &[
            0x0000000000000000,
            0x3fd0000000000000,
            0x3fe0000000000000,
            0x3fe8000000000000,
            0x3ff0000000000000,
        ],
    );
}

#[test]
fn s03_project_nonzero_initial_state_and_sparse_negative_updates() {
    let result = run(plan(0.25, 4, vec![channel(1, 2, 4.0, -2.0)]));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.last_committed_tick, Some(4));
    assert_trace(
        &result,
        0,
        1,
        &[0, 2, 4],
        &[0x4010000000000000, 0x4000000000000000, 0x0000000000000000],
    );
    assert_times_bits(
        &result,
        0,
        &[0x0000000000000000, 0x3fe0000000000000, 0x3ff0000000000000],
    );
}

#[test]
fn s04_project_cancel_before_initial_tick_preserves_empty_traces() {
    let mut run = PreparedRunV1::prepare(s01_plan(), small_limits()).unwrap();
    assert_eq!(
        run.advance(ControlV1::Cancel),
        Ok(AdvanceV1::Terminal(TerminalStatusV1::Cancelled))
    );
    let result = run.into_result().unwrap();
    assert_eq!(result.status, TerminalStatusV1::Cancelled);
    assert_eq!(result.last_committed_tick, None);
    assert_trace(&result, 0, 1, &[], &[]);
    assert_trace(&result, 1, 2, &[], &[]);
}

#[test]
fn s05_project_cancel_at_tick_three_keeps_only_committed_prefix() {
    let mut run = PreparedRunV1::prepare(s01_plan(), small_limits()).unwrap();
    for tick in 0..3 {
        assert_eq!(
            run.advance(ControlV1::Continue),
            Ok(AdvanceV1::Committed { tick })
        );
    }
    assert_eq!(run.next_tick(), Some(3));
    run.advance(ControlV1::Cancel).unwrap();
    let result = run.into_result().unwrap();
    assert_eq!(result.status, TerminalStatusV1::Cancelled);
    assert_eq!(result.last_committed_tick, Some(2));
    assert_trace(
        &result,
        0,
        1,
        &[0, 1, 2],
        &[0x0000000000000000, 0x3ff0000000000000, 0x4000000000000000],
    );
    assert_trace(
        &result,
        1,
        2,
        &[0, 2],
        &[0x0000000000000000, 0x4024000000000000],
    );
}

#[test]
fn s06_project_overflow_rolls_back_every_channel_at_tick_zero() {
    let result = run(plan(
        1.0,
        1,
        vec![channel(1, 1, 0.0, 1.0), channel(2, 1, f64::MAX, f64::MAX)],
    ));
    assert_eq!(
        result.status,
        TerminalStatusV1::Failed {
            tick: 0,
            reason: FailureReasonV1::NonfiniteUpdate {
                channel_id: 2,
                state_bits: 0x7fefffffffffffff,
                increment_bits: 0x7fefffffffffffff,
                result_bits: 0x7ff0000000000000,
            },
        }
    );
    assert_eq!(result.last_committed_tick, None);
    assert_trace(&result, 0, 1, &[], &[]);
    assert_trace(&result, 1, 2, &[], &[]);
}

#[test]
fn s07_project_one_sample_avoids_unused_overflowing_addition() {
    let result = run(plan(1.0, 0, vec![channel(1, 1, f64::MAX, f64::MAX)]));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.last_committed_tick, Some(0));
    assert_trace(&result, 0, 1, &[0], &[0x7fefffffffffffff]);
    assert_times_bits(&result, 0, &[0x0000000000000000]);
}

#[test]
fn s08_project_time_overflow_fails_before_tick_two_output() {
    let result = run(plan(f64::MAX, 2, vec![channel(1, 1, 0.0, 1.0)]));
    assert_eq!(
        result.status,
        TerminalStatusV1::Failed {
            tick: 2,
            reason: FailureReasonV1::NonfiniteTime {
                bits: 0x7ff0000000000000
            },
        }
    );
    assert_eq!(result.last_committed_tick, Some(1));
    assert_trace(
        &result,
        0,
        1,
        &[0, 1],
        &[0x0000000000000000, 0x3ff0000000000000],
    );
    assert_times_bits(&result, 0, &[0x0000000000000000, 0x7fefffffffffffff]);
}

#[test]
fn s09_project_separate_runs_own_state_and_fresh_prepare_resets() {
    let original = plan(0.25, 4, vec![channel(1, 1, 0.0, 1.0)]);
    let first = run(original.clone());
    let mut changed = original.clone();
    changed.channels[0].increment = 2.0;
    let second = run(changed);
    assert_trace(
        &first,
        0,
        1,
        &[0, 1, 2, 3, 4],
        &[
            0x0000000000000000,
            0x3ff0000000000000,
            0x4000000000000000,
            0x4008000000000000,
            0x4010000000000000,
        ],
    );
    assert_trace(
        &second,
        0,
        1,
        &[0, 1, 2, 3, 4],
        &[
            0x0000000000000000,
            0x4000000000000000,
            0x4010000000000000,
            0x4018000000000000,
            0x4020000000000000,
        ],
    );
    drop(second);
    assert_eq!(first, run(original));
}

#[test]
fn s10_project_terminal_continue_and_cancel_leave_result_identical() {
    let mut run =
        PreparedRunV1::prepare(plan(1.0, 0, vec![channel(1, 1, 3.0, 2.0)]), small_limits())
            .unwrap();
    assert_eq!(
        run.advance(ControlV1::Continue),
        Ok(AdvanceV1::Terminal(TerminalStatusV1::Completed))
    );
    let before_state = run.channels[0].state.to_bits();
    let before_cursor = run.next_tick;
    for control in [ControlV1::Continue, ControlV1::Cancel] {
        assert_eq!(
            run.advance(control),
            Err(AdvanceErrorV1::AlreadyTerminal {
                status: TerminalStatusV1::Completed
            })
        );
        assert_eq!(run.channels[0].state.to_bits(), before_state);
        assert_eq!(run.next_tick, before_cursor);
        assert_eq!(run.traces()[0].samples().len(), 1);
    }
    let result = run.into_result().unwrap();
    assert_eq!(result.last_committed_tick, Some(0));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_trace(&result, 0, 1, &[0], &[0x4008000000000000]);
}

fn invalid_without_reservation(plan: PlanV1) -> PrepareErrorV1 {
    prepare_with_reservation(plan, small_limits(), |_| {
        panic!("invalid plan reached reservation")
    })
    .unwrap_err()
}

#[test]
fn invalid_inputs_reject_before_reservation_and_preserve_bits() {
    assert!(matches!(
        invalid_without_reservation(plan(1.0, 0, vec![])),
        PrepareErrorV1::EmptyChannels
    ));
    for step in [
        0.0,
        -0.0,
        -1.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8000000000042),
    ] {
        assert!(matches!(
            invalid_without_reservation(plan(step, 0, vec![channel(1, 1, 0.0, 1.0)])),
            PrepareErrorV1::InvalidBaseStep { bits } if bits == step.to_bits()
        ));
    }
    assert!(matches!(
        invalid_without_reservation(plan(1.0, 0, vec![channel(9, 0, 0.0, 1.0)])),
        PrepareErrorV1::ZeroPeriodTicks { channel_id: 9 }
    ));
    assert!(matches!(
        invalid_without_reservation(plan(
            1.0,
            0,
            vec![channel(9, 1, 0.0, 1.0), channel(9, 2, 2.0, 3.0)]
        )),
        PrepareErrorV1::DuplicateChannelId {
            id: 9,
            first: 0,
            second: 1
        }
    ));
    for value in [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8000000000042),
    ] {
        assert!(matches!(
            invalid_without_reservation(plan(1.0, 0, vec![channel(9, 1, value, 1.0)])),
            PrepareErrorV1::NonfiniteInitialState { channel_id: 9, bits } if bits == value.to_bits()
        ));
        assert!(matches!(
            invalid_without_reservation(plan(1.0, 0, vec![channel(9, 1, 0.0, value)])),
            PrepareErrorV1::NonfiniteIncrement { channel_id: 9, bits } if bits == value.to_bits()
        ));
    }
    for last_tick in [MAX_LAST_TICK_V1 + 1, u64::MAX] {
        assert!(matches!(
            invalid_without_reservation(plan(1.0, last_tick, vec![channel(1, 1, 0.0, 1.0)])),
            PrepareErrorV1::LastTickOutOfRange { actual, maximum: MAX_LAST_TICK_V1 } if actual == last_tick
        ));
    }
}

#[test]
fn s01_exact_requirements_and_each_limit_one_below_rejects_before_reservation() {
    let required = RequirementsV1 {
        channels: 2,
        base_ticks: 7,
        channel_visits: 14,
        state_additions: 9,
        trace_samples: 11,
        trace_payload_bytes: 11 * size_of::<SampleV1>() as u64,
    };
    let exact = LimitsV1 {
        max_channels: required.channels,
        max_base_ticks: required.base_ticks,
        max_channel_visits: required.channel_visits,
        max_state_additions: required.state_additions,
        max_trace_samples: required.trace_samples,
        max_trace_payload_bytes: required.trace_payload_bytes,
    };
    let prepared = PreparedRunV1::prepare(s01_plan(), exact).unwrap();
    assert_eq!(prepared.requirements(), required);
    assert_eq!(finish(prepared).status, TerminalStatusV1::Completed);

    for (resource, expected_required) in [
        (ResourceV1::Channels, required.channels),
        (ResourceV1::BaseTicks, required.base_ticks),
        (ResourceV1::ChannelVisits, required.channel_visits),
        (ResourceV1::StateAdditions, required.state_additions),
        (ResourceV1::TraceSamples, required.trace_samples),
        (ResourceV1::TracePayloadBytes, required.trace_payload_bytes),
    ] {
        let mut limits = exact;
        let lower = expected_required - 1;
        match resource {
            ResourceV1::Channels => limits.max_channels = lower,
            ResourceV1::BaseTicks => limits.max_base_ticks = lower,
            ResourceV1::ChannelVisits => limits.max_channel_visits = lower,
            ResourceV1::StateAdditions => limits.max_state_additions = lower,
            ResourceV1::TraceSamples => limits.max_trace_samples = lower,
            ResourceV1::TracePayloadBytes => limits.max_trace_payload_bytes = lower,
        }
        let error = prepare_with_reservation(s01_plan(), limits, |_| {
            panic!("limit failure reached reservation")
        })
        .unwrap_err();
        assert!(
            matches!(error, PrepareErrorV1::LimitExceeded { resource: actual, required, limit }
            if actual == resource && required == expected_required && limit == lower)
        );
    }
}

#[test]
fn checked_count_overflow_rejects_without_allocating_a_huge_run() {
    // 2048 * 2^53 base ticks overflows u64, using only 2048 input headers.
    let channels = (0..2048)
        .map(|id| channel(id, u64::MAX, 0.0, 1.0))
        .collect();
    let error = prepare_with_reservation(
        plan(1.0, MAX_LAST_TICK_V1, channels),
        unrestricted_counts(),
        |_| panic!("overflow reached reservation"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PrepareErrorV1::CountOverflow {
            resource: ResourceV1::ChannelVisits
        }
    ));

    // Logical counts fit, but 128 * 2^53 samples * size_of::<SampleV1>() does not.
    let channels = (0..128).map(|id| channel(id, 1, 0.0, 1.0)).collect();
    let error = prepare_with_reservation(
        plan(1.0, MAX_LAST_TICK_V1, channels),
        unrestricted_counts(),
        |_| panic!("payload overflow reached reservation"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        PrepareErrorV1::CountOverflow {
            resource: ResourceV1::TracePayloadBytes
        }
    ));
    assert!(matches!(
        checked_add(u64::MAX, 1, ResourceV1::TraceSamples),
        Err(PrepareErrorV1::CountOverflow {
            resource: ResourceV1::TraceSamples
        })
    ));
}

fn capacity_error() -> TryReserveError {
    // Guaranteed capacity-layout error; never attempts the requested allocation.
    Vec::<u8>::new().try_reserve_exact(usize::MAX).unwrap_err()
}

#[test]
fn every_reservation_site_preserves_structured_failure_and_stops_preparation() {
    let targets = [
        (StorageV1::States, 2, 2 * size_of::<ChannelStateV1>() as u64),
        (StorageV1::Scratch, 2, 2 * size_of::<StagedV1>() as u64),
        (
            StorageV1::TraceHeaders,
            2,
            2 * size_of::<ChannelTraceV1>() as u64,
        ),
        (
            StorageV1::TraceSamples { channel_id: 1 },
            7,
            7 * size_of::<SampleV1>() as u64,
        ),
        (
            StorageV1::TraceSamples { channel_id: 2 },
            4,
            4 * size_of::<SampleV1>() as u64,
        ),
    ];
    for failed_index in 0..targets.len() {
        let calls = Cell::new(0);
        let error = prepare_with_reservation(s01_plan(), small_limits(), |request| {
            let index = calls.get();
            calls.set(index + 1);
            assert_eq!(
                (
                    request.storage,
                    request.requested_elements,
                    request.requested_payload_bytes
                ),
                targets[index]
            );
            if index == failed_index {
                Err(capacity_error())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(calls.get(), failed_index + 1);
        assert!(std::error::Error::source(&error).is_some());
        match error {
            PrepareErrorV1::ReservationFailed {
                storage,
                requested_elements,
                requested_payload_bytes,
                ..
            } => {
                assert_eq!(
                    (storage, requested_elements, requested_payload_bytes),
                    targets[failed_index]
                );
            }
            other => panic!("wrong reservation error: {other:?}"),
        }
    }
}

#[test]
fn host_addressability_rejects_before_reservation_hook() {
    let mut buffer = Vec::<SampleV1>::new();
    let elements = (isize::MAX as u64 / size_of::<SampleV1>() as u64) + 1;
    let error = reserve(
        &mut buffer,
        elements,
        StorageV1::TraceSamples { channel_id: 1 },
        &mut |_| panic!("unaddressable storage reached reservation"),
    )
    .unwrap_err();
    assert!(matches!(error, PrepareErrorV1::AddressSpaceExceeded {
        storage: StorageV1::TraceSamples { channel_id: 1 }, requested_elements, ..
    } if requested_elements == elements));
    assert_eq!(buffer.capacity(), 0);
}

#[test]
fn private_clock_rejects_duplicate_decreasing_and_nonfinite_times_without_large_run() {
    for previous in [1.0_f64, 2.0] {
        assert_eq!(
            clock_time(1, 1.0, Some(previous)),
            Err(FailureReasonV1::NonIncreasingTime {
                previous_bits: previous.to_bits(),
                current_bits: 1.0_f64.to_bits(),
            })
        );
    }
    assert_eq!(
        clock_time(2, f64::MAX, Some(f64::MAX)),
        Err(FailureReasonV1::NonfiniteTime {
            bits: 0x7ff0000000000000
        })
    );
    assert_eq!(clock_time(0, f64::from_bits(1), None).unwrap().to_bits(), 0);
    assert_eq!(
        clock_time(1, f64::from_bits(1), Some(0.0))
            .unwrap()
            .to_bits(),
        1
    );
}

#[test]
fn overflow_after_valid_prefix_rolls_back_outputs_and_all_stored_states() {
    let mut run = PreparedRunV1::prepare(
        plan(
            1.0,
            2,
            vec![
                channel(1, 1, 0.0, 1.0),
                channel(2, 1, f64::MAX / 2.0, f64::MAX / 2.0),
            ],
        ),
        small_limits(),
    )
    .unwrap();
    assert_eq!(
        run.advance(ControlV1::Continue),
        Ok(AdvanceV1::Committed { tick: 0 })
    );
    let before = [
        run.channels[0].state.to_bits(),
        run.channels[1].state.to_bits(),
    ];
    let status = run.advance(ControlV1::Continue).unwrap();
    assert!(matches!(
        status,
        AdvanceV1::Terminal(TerminalStatusV1::Failed {
            tick: 1,
            reason: FailureReasonV1::NonfiniteUpdate { channel_id: 2, .. }
        })
    ));
    assert_eq!(
        [
            run.channels[0].state.to_bits(),
            run.channels[1].state.to_bits()
        ],
        before
    );
    assert_eq!(run.last_committed_tick(), Some(0));
    assert_eq!(run.traces()[0].samples().len(), 1);
    assert_eq!(run.traces()[1].samples().len(), 1);
    assert_eq!(run.next_tick, 1);
    let failure = run.status().unwrap();
    assert_eq!(
        run.advance(ControlV1::Cancel),
        Err(AdvanceErrorV1::AlreadyTerminal { status: failure })
    );
    assert_eq!(run.into_result().unwrap().last_committed_tick, Some(0));
}

#[test]
fn final_channel_hit_before_global_endpoint_does_not_perform_unused_addition() {
    let result = run(plan(
        1.0,
        3,
        vec![channel(1, 2, f64::MAX / 2.0, f64::MAX / 2.0)],
    ));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.last_committed_tick, Some(3));
    assert_trace(
        &result,
        0,
        1,
        &[0, 2],
        &[0x7fdfffffffffffff, 0x7fefffffffffffff],
    );
}

#[test]
fn recurrence_uses_primitive_additions_instead_of_closed_form_reassociation() {
    let result = run(plan(
        1.0,
        2,
        vec![channel(1, 1, 9_007_199_254_740_992.0, 1.0)],
    ));
    assert_trace(
        &result,
        0,
        1,
        &[0, 1, 2],
        &[0x4340000000000000, 0x4340000000000000, 0x4340000000000000],
    );
}

#[test]
fn plan_order_is_trace_order_and_nonhit_base_ticks_still_commit() {
    let result = run(plan(
        0.5,
        3,
        vec![channel(9, 10, -0.0, f64::MAX), channel(2, 10, 4.0, 1.0)],
    ));
    assert_eq!(result.status, TerminalStatusV1::Completed);
    assert_eq!(result.last_committed_tick, Some(3));
    assert_trace(&result, 0, 9, &[0], &[0x8000000000000000]);
    assert_trace(&result, 1, 2, &[0], &[0x4010000000000000]);
}

#[test]
fn unfinished_consumption_returns_run_ownership_and_cancelled_terminal_is_stable() {
    let mut run = PreparedRunV1::prepare(s01_plan(), small_limits()).unwrap();
    run.advance(ControlV1::Continue).unwrap();
    let mut run = run.into_result().unwrap_err().into_run();
    assert_eq!(run.last_committed_tick(), Some(0));
    assert_eq!(run.next_tick(), Some(1));
    run.advance(ControlV1::Cancel).unwrap();
    assert_eq!(run.next_tick(), None);
    for control in [ControlV1::Continue, ControlV1::Cancel] {
        assert_eq!(
            run.advance(control),
            Err(AdvanceErrorV1::AlreadyTerminal {
                status: TerminalStatusV1::Cancelled
            })
        );
    }
    let result = run.into_result().unwrap();
    assert_eq!(result.status, TerminalStatusV1::Cancelled);
    assert_eq!(result.last_committed_tick, Some(0));
    assert_trace(&result, 0, 1, &[0], &[0]);
    assert_trace(&result, 1, 2, &[0], &[0]);
}

#[test]
fn advancing_preserves_every_prepared_vector_capacity_and_storage_address() {
    let mut run = PreparedRunV1::prepare(s01_plan(), small_limits()).unwrap();
    let capacities = [
        run.channels.capacity(),
        run.scratch.capacity(),
        run.traces.capacity(),
        run.traces[0].samples.capacity(),
        run.traces[1].samples.capacity(),
    ];
    let addresses = [
        run.channels.as_ptr() as usize,
        run.scratch.as_ptr() as usize,
        run.traces.as_ptr() as usize,
        run.traces[0].samples.as_ptr() as usize,
        run.traces[1].samples.as_ptr() as usize,
    ];
    for _ in 0..7 {
        run.advance(ControlV1::Continue).unwrap();
        assert_eq!(
            [
                run.channels.capacity(),
                run.scratch.capacity(),
                run.traces.capacity(),
                run.traces[0].samples.capacity(),
                run.traces[1].samples.capacity()
            ],
            capacities
        );
        assert_eq!(
            [
                run.channels.as_ptr() as usize,
                run.scratch.as_ptr() as usize,
                run.traces.as_ptr() as usize,
                run.traces[0].samples.as_ptr() as usize,
                run.traces[1].samples.as_ptr() as usize
            ],
            addresses
        );
    }
    assert_eq!(run.status(), Some(TerminalStatusV1::Completed));
}
