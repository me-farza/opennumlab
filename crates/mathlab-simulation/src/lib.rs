// SPDX-License-Identifier: GPL-3.0-or-later
//! SIM-PREP-01: independent scalar Constant -> Sum -> Unit Delay channels.
//!
//! This is original project policy, not a graph interpreter, continuous solver,
//! MATLAB backend, or claim about native hidden endpoint state. Preparation
//! reserves every run-owned buffer. `advance` allocates no storage and commits
//! each base tick atomically. The public API exposes outputs, never hidden state.

#![forbid(unsafe_code)]

use std::collections::TryReserveError;
use std::fmt;
use std::mem::size_of;

pub const PROFILE_V1: &str = "SIM-PREP-01-DISCRETE-ACCUMULATORS-V1";
pub const MAX_LAST_TICK_V1: u64 = (1_u64 << 53) - 1;

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelPlanV1 {
    pub id: u64,
    pub period_ticks: u64,
    pub initial_state: f64,
    pub increment: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanV1 {
    pub revision: u64,
    pub base_step: f64,
    pub last_tick: u64,
    pub channels: Vec<ChannelPlanV1>,
}

/// Explicit limits; there is deliberately no default resource budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitsV1 {
    pub max_channels: u64,
    pub max_base_ticks: u64,
    pub max_channel_visits: u64,
    pub max_state_additions: u64,
    pub max_trace_samples: u64,
    pub max_trace_payload_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequirementsV1 {
    pub channels: u64,
    pub base_ticks: u64,
    pub channel_visits: u64,
    pub state_additions: u64,
    pub trace_samples: u64,
    /// Requested SampleV1 payload only: no allocator padding or RSS guarantee.
    pub trace_payload_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceV1 {
    Channels,
    BaseTicks,
    ChannelVisits,
    StateAdditions,
    TraceSamples,
    TracePayloadBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageV1 {
    States,
    Scratch,
    TraceHeaders,
    TraceSamples { channel_id: u64 },
}

#[derive(Debug)]
pub enum PrepareErrorV1 {
    EmptyChannels,
    InvalidBaseStep {
        bits: u64,
    },
    LastTickOutOfRange {
        actual: u64,
        maximum: u64,
    },
    DuplicateChannelId {
        id: u64,
        first: usize,
        second: usize,
    },
    ZeroPeriodTicks {
        channel_id: u64,
    },
    NonfiniteInitialState {
        channel_id: u64,
        bits: u64,
    },
    NonfiniteIncrement {
        channel_id: u64,
        bits: u64,
    },
    CountOverflow {
        resource: ResourceV1,
    },
    LimitExceeded {
        resource: ResourceV1,
        required: u64,
        limit: u64,
    },
    AddressSpaceExceeded {
        storage: StorageV1,
        requested_elements: u64,
        element_width_bytes: u64,
    },
    ReservationFailed {
        storage: StorageV1,
        requested_elements: u64,
        requested_payload_bytes: u64,
        cause: TryReserveError,
    },
}

impl fmt::Display for PrepareErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SIM-PREP-01 preparation failed: {self:?}")
    }
}

impl std::error::Error for PrepareErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReservationFailed { cause, .. } => Some(cause),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampleV1 {
    pub tick: u64,
    pub time: f64,
    pub value: f64,
}

#[derive(Debug, PartialEq)]
pub struct ChannelTraceV1 {
    pub channel_id: u64,
    pub period_ticks: u64,
    samples: Vec<SampleV1>,
}

impl ChannelTraceV1 {
    pub fn samples(&self) -> &[SampleV1] {
        &self.samples
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlV1 {
    Continue,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReasonV1 {
    NonfiniteTime {
        bits: u64,
    },
    NonIncreasingTime {
        previous_bits: u64,
        current_bits: u64,
    },
    NonfiniteUpdate {
        channel_id: u64,
        state_bits: u64,
        increment_bits: u64,
        result_bits: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatusV1 {
    Completed,
    Cancelled,
    Failed { tick: u64, reason: FailureReasonV1 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceV1 {
    Committed { tick: u64 },
    Terminal(TerminalStatusV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvanceErrorV1 {
    AlreadyTerminal { status: TerminalStatusV1 },
}

impl fmt::Display for AdvanceErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SIM-PREP-01 run is already terminal: {self:?}")
    }
}

impl std::error::Error for AdvanceErrorV1 {}

#[derive(Debug, PartialEq)]
pub struct RunResultV1 {
    revision: u64,
    profile: &'static str,
    pub base_step: f64,
    pub last_tick: u64,
    pub last_committed_tick: Option<u64>,
    pub status: TerminalStatusV1,
    pub traces: Vec<ChannelTraceV1>,
}

impl RunResultV1 {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn profile(&self) -> &'static str {
        self.profile
    }
}

#[derive(Debug)]
struct ChannelStateV1 {
    id: u64,
    period_ticks: u64,
    state: f64,
    increment: f64,
}

#[derive(Debug, Clone, Copy)]
struct StagedV1 {
    due: bool,
    next_state: Option<f64>,
}

#[derive(Debug)]
pub struct PreparedRunV1 {
    revision: u64,
    base_step: f64,
    last_tick: u64,
    requirements: RequirementsV1,
    channels: Vec<ChannelStateV1>,
    scratch: Vec<StagedV1>,
    traces: Vec<ChannelTraceV1>,
    next_tick: u64,
    previous_time: Option<f64>,
    terminal: Option<TerminalStatusV1>,
}

/// Consuming an unfinished run returns its ownership so the caller can resume.
#[derive(Debug)]
pub struct UnfinishedRunV1 {
    run: PreparedRunV1,
}

impl UnfinishedRunV1 {
    pub fn into_run(self) -> PreparedRunV1 {
        self.run
    }
}

impl fmt::Display for UnfinishedRunV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SIM-PREP-01 result requires a terminal run")
    }
}

impl std::error::Error for UnfinishedRunV1 {}

impl PreparedRunV1 {
    pub fn prepare(plan: PlanV1, limits: LimitsV1) -> Result<Self, PrepareErrorV1> {
        prepare_with_reservation(plan, limits, |_| Ok(()))
    }

    pub fn requirements(&self) -> RequirementsV1 {
        self.requirements
    }

    pub fn traces(&self) -> &[ChannelTraceV1] {
        &self.traces
    }

    pub fn status(&self) -> Option<TerminalStatusV1> {
        self.terminal
    }

    pub fn next_tick(&self) -> Option<u64> {
        if self.terminal.is_some() {
            None
        } else {
            Some(self.next_tick)
        }
    }

    pub fn last_committed_tick(&self) -> Option<u64> {
        self.next_tick.checked_sub(1)
    }

    /// Visit one base tick or cancel at its boundary. No callback is invoked.
    ///
    /// The only pushes use capacities reserved for every possible channel hit.
    /// There are no reservations, clones, formatting, or dynamic containers here.
    pub fn advance(&mut self, control: ControlV1) -> Result<AdvanceV1, AdvanceErrorV1> {
        if let Some(status) = self.terminal {
            return Err(AdvanceErrorV1::AlreadyTerminal { status });
        }
        if control == ControlV1::Cancel {
            return Ok(self.terminate(TerminalStatusV1::Cancelled));
        }

        let tick = self.next_tick;
        let time = match clock_time(tick, self.base_step, self.previous_time) {
            Ok(time) => time,
            Err(reason) => {
                return Ok(self.terminate(TerminalStatusV1::Failed { tick, reason }));
            }
        };

        // Stage from the old state of every independent channel. No trace or
        // state is modified until all due additions have passed validation.
        for index in 0..self.channels.len() {
            let channel = &self.channels[index];
            let due = tick % channel.period_ticks == 0;
            let next_state = if due && self.last_tick - tick >= channel.period_ticks {
                let next = channel.state + channel.increment;
                if !next.is_finite() {
                    let reason = FailureReasonV1::NonfiniteUpdate {
                        channel_id: channel.id,
                        state_bits: channel.state.to_bits(),
                        increment_bits: channel.increment.to_bits(),
                        result_bits: next.to_bits(),
                    };
                    return Ok(self.terminate(TerminalStatusV1::Failed { tick, reason }));
                }
                Some(next)
            } else {
                None
            };
            self.scratch[index] = StagedV1 { due, next_state };
        }

        for index in 0..self.channels.len() {
            let staged = self.scratch[index];
            if staged.due {
                let channel = &mut self.channels[index];
                let samples = &mut self.traces[index].samples;
                // A channel has exactly last_tick / period_ticks + 1 hits.
                // That complete capacity was reserved before the first tick.
                debug_assert!(samples.len() < samples.capacity());
                samples.push(SampleV1 {
                    tick,
                    time,
                    value: channel.state,
                });
                if let Some(next) = staged.next_state {
                    channel.state = next;
                }
            }
        }
        self.previous_time = Some(time);
        // Preflight admits at most 2^53 base ticks, so this cannot overflow.
        self.next_tick = tick + 1;
        if tick == self.last_tick {
            Ok(self.terminate(TerminalStatusV1::Completed))
        } else {
            Ok(AdvanceV1::Committed { tick })
        }
    }

    fn terminate(&mut self, status: TerminalStatusV1) -> AdvanceV1 {
        self.terminal = Some(status);
        AdvanceV1::Terminal(status)
    }

    pub fn into_result(self) -> Result<RunResultV1, UnfinishedRunV1> {
        let Some(status) = self.terminal else {
            return Err(UnfinishedRunV1 { run: self });
        };
        Ok(RunResultV1 {
            revision: self.revision,
            profile: PROFILE_V1,
            base_step: self.base_step,
            last_tick: self.last_tick,
            last_committed_tick: self.last_committed_tick(),
            status,
            traces: self.traces,
        })
    }
}

fn clock_time(tick: u64, base_step: f64, previous: Option<f64>) -> Result<f64, FailureReasonV1> {
    // No cumulative addition, floating remainder, or stop-time division.
    let time = (tick as f64) * base_step;
    if !time.is_finite() {
        return Err(FailureReasonV1::NonfiniteTime {
            bits: time.to_bits(),
        });
    }
    if let Some(previous) = previous {
        if time <= previous {
            return Err(FailureReasonV1::NonIncreasingTime {
                previous_bits: previous.to_bits(),
                current_bits: time.to_bits(),
            });
        }
    }
    Ok(time)
}

fn checked_add(lhs: u64, rhs: u64, resource: ResourceV1) -> Result<u64, PrepareErrorV1> {
    lhs.checked_add(rhs)
        .ok_or(PrepareErrorV1::CountOverflow { resource })
}

fn checked_mul(lhs: u64, rhs: u64, resource: ResourceV1) -> Result<u64, PrepareErrorV1> {
    lhs.checked_mul(rhs)
        .ok_or(PrepareErrorV1::CountOverflow { resource })
}

fn check_limit(resource: ResourceV1, required: u64, limit: u64) -> Result<(), PrepareErrorV1> {
    if required > limit {
        Err(PrepareErrorV1::LimitExceeded {
            resource,
            required,
            limit,
        })
    } else {
        Ok(())
    }
}

/// Allocation-free input/count validation. Duplicate detection uses a bounded
/// quadratic scan, avoiding allocation before invalid input has been rejected.
fn preflight(plan: &PlanV1, limits: LimitsV1) -> Result<RequirementsV1, PrepareErrorV1> {
    if plan.channels.is_empty() {
        return Err(PrepareErrorV1::EmptyChannels);
    }
    if !plan.base_step.is_finite() || plan.base_step <= 0.0 {
        return Err(PrepareErrorV1::InvalidBaseStep {
            bits: plan.base_step.to_bits(),
        });
    }
    if plan.last_tick > MAX_LAST_TICK_V1 {
        return Err(PrepareErrorV1::LastTickOutOfRange {
            actual: plan.last_tick,
            maximum: MAX_LAST_TICK_V1,
        });
    }
    let channels =
        u64::try_from(plan.channels.len()).map_err(|_| PrepareErrorV1::CountOverflow {
            resource: ResourceV1::Channels,
        })?;
    check_limit(ResourceV1::Channels, channels, limits.max_channels)?;
    for (index, channel) in plan.channels.iter().enumerate() {
        if channel.period_ticks == 0 {
            return Err(PrepareErrorV1::ZeroPeriodTicks {
                channel_id: channel.id,
            });
        }
        if !channel.initial_state.is_finite() {
            return Err(PrepareErrorV1::NonfiniteInitialState {
                channel_id: channel.id,
                bits: channel.initial_state.to_bits(),
            });
        }
        if !channel.increment.is_finite() {
            return Err(PrepareErrorV1::NonfiniteIncrement {
                channel_id: channel.id,
                bits: channel.increment.to_bits(),
            });
        }
        for first in 0..index {
            if plan.channels[first].id == channel.id {
                return Err(PrepareErrorV1::DuplicateChannelId {
                    id: channel.id,
                    first,
                    second: index,
                });
            }
        }
    }

    let base_ticks = checked_add(plan.last_tick, 1, ResourceV1::BaseTicks)?;
    let channel_visits = checked_mul(channels, base_ticks, ResourceV1::ChannelVisits)?;
    let mut trace_samples = 0;
    let mut state_additions = 0;
    for channel in &plan.channels {
        let updates = plan.last_tick / channel.period_ticks;
        let samples = checked_add(updates, 1, ResourceV1::TraceSamples)?;
        trace_samples = checked_add(trace_samples, samples, ResourceV1::TraceSamples)?;
        state_additions = checked_add(state_additions, updates, ResourceV1::StateAdditions)?;
    }
    let trace_payload_bytes = checked_mul(
        trace_samples,
        size_of::<SampleV1>() as u64,
        ResourceV1::TracePayloadBytes,
    )?;
    for (resource, required, limit) in [
        (ResourceV1::BaseTicks, base_ticks, limits.max_base_ticks),
        (
            ResourceV1::ChannelVisits,
            channel_visits,
            limits.max_channel_visits,
        ),
        (
            ResourceV1::StateAdditions,
            state_additions,
            limits.max_state_additions,
        ),
        (
            ResourceV1::TraceSamples,
            trace_samples,
            limits.max_trace_samples,
        ),
        (
            ResourceV1::TracePayloadBytes,
            trace_payload_bytes,
            limits.max_trace_payload_bytes,
        ),
    ] {
        check_limit(resource, required, limit)?;
    }
    Ok(RequirementsV1 {
        channels,
        base_ticks,
        channel_visits,
        state_additions,
        trace_samples,
        trace_payload_bytes,
    })
}

#[derive(Debug, Clone, Copy)]
struct ReservationV1 {
    storage: StorageV1,
    requested_elements: u64,
    requested_payload_bytes: u64,
}

fn reserve<T, F>(
    buffer: &mut Vec<T>,
    elements: u64,
    storage: StorageV1,
    before_reservation: &mut F,
) -> Result<(), PrepareErrorV1>
where
    F: FnMut(ReservationV1) -> Result<(), TryReserveError>,
{
    let width = size_of::<T>() as u64;
    let address_error = || PrepareErrorV1::AddressSpaceExceeded {
        storage,
        requested_elements: elements,
        element_width_bytes: width,
    };
    let count = usize::try_from(elements).map_err(|_| address_error())?;
    let bytes = elements.checked_mul(width).ok_or_else(address_error)?;
    if bytes > isize::MAX as u64 {
        return Err(address_error());
    }
    let request = ReservationV1 {
        storage,
        requested_elements: elements,
        requested_payload_bytes: bytes,
    };
    before_reservation(request).map_err(|cause| PrepareErrorV1::ReservationFailed {
        storage: request.storage,
        requested_elements: request.requested_elements,
        requested_payload_bytes: request.requested_payload_bytes,
        cause,
    })?;
    buffer
        .try_reserve_exact(count)
        .map_err(|cause| PrepareErrorV1::ReservationFailed {
            storage: request.storage,
            requested_elements: request.requested_elements,
            requested_payload_bytes: request.requested_payload_bytes,
            cause,
        })
}

fn prepare_with_reservation<F>(
    plan: PlanV1,
    limits: LimitsV1,
    mut before_reservation: F,
) -> Result<PreparedRunV1, PrepareErrorV1>
where
    F: FnMut(ReservationV1) -> Result<(), TryReserveError>,
{
    let requirements = preflight(&plan, limits)?;
    let mut states = Vec::new();
    let mut scratch = Vec::new();
    let mut traces = Vec::new();
    reserve(
        &mut states,
        requirements.channels,
        StorageV1::States,
        &mut before_reservation,
    )?;
    reserve(
        &mut scratch,
        requirements.channels,
        StorageV1::Scratch,
        &mut before_reservation,
    )?;
    reserve(
        &mut traces,
        requirements.channels,
        StorageV1::TraceHeaders,
        &mut before_reservation,
    )?;

    let PlanV1 {
        revision,
        base_step,
        last_tick,
        channels,
    } = plan;
    for channel in channels {
        let mut samples = Vec::new();
        // The same checked quantity was admitted in preflight.
        let count = checked_add(
            last_tick / channel.period_ticks,
            1,
            ResourceV1::TraceSamples,
        )?;
        reserve(
            &mut samples,
            count,
            StorageV1::TraceSamples {
                channel_id: channel.id,
            },
            &mut before_reservation,
        )?;
        traces.push(ChannelTraceV1 {
            channel_id: channel.id,
            period_ticks: channel.period_ticks,
            samples,
        });
        states.push(ChannelStateV1 {
            id: channel.id,
            period_ticks: channel.period_ticks,
            state: channel.initial_state,
            increment: channel.increment,
        });
        scratch.push(StagedV1 {
            due: false,
            next_state: None,
        });
    }
    Ok(PreparedRunV1 {
        revision,
        base_step,
        last_tick,
        requirements,
        channels: states,
        scratch,
        traces,
        next_tick: 0,
        previous_time: None,
        terminal: None,
    })
}

#[cfg(test)]
mod tests;
