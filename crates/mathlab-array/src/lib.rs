// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned dense 2-D real-double arrays and bounded arithmetic.
//!
//! Construction and addition retain the accepted RN-PREP-01 policy.
//! It supplies no parser, worker route, or backend selection.

#![forbid(unsafe_code)]

mod dense_double2d;

pub use dense_double2d::{ArrayErrorV1, DenseDouble2d, ErrorCodeV1, ErrorContextV1, LimitsV1};
pub use dense_double2d::{MtimesErrorV1, MtimesWorkBudgetV1};

#[cfg(test)]
mod tests;
