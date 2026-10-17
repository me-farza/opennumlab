// SPDX-License-Identifier: GPL-3.0-or-later
//! Consuming real-double sign transformation; no new layout or allocation.

use super::DenseDouble2d;

impl DenseDouble2d {
    /// Negate this owned payload in place and return the same owned allocation.
    ///
    /// Project bit policy: toggle only the binary64 sign bit of every element.
    /// Shape, column-major order and all other bits are unchanged, including
    /// signed-zero and NaN payload bits. This is an explicit project operation,
    /// not a claim about MATLAB NaNs, floating-point exceptions or performance.
    /// No allocation, cloning, new layout, limits or work budget is needed.
    pub fn into_negated(mut self) -> Self {
        for element in &mut self.data {
            *element = f64::from_bits(element.to_bits() ^ (1_u64 << 63));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LimitsV1;

    #[test]
    fn consuming_negation_keeps_allocation_shape_order_and_project_bits() {
        // Fixed words define this project's operation, not a native oracle.
        let input_bits = [
            0x0000000000000000_u64,
            0x8000000000000000,
            0x3ff0000000000000,
            0xc000000000000000,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8000000000042,
            0xfff8000000001234,
        ];
        let expected = [
            0x8000000000000000_u64,
            0x0000000000000000,
            0xbff0000000000000,
            0x4000000000000000,
            0xfff0000000000000,
            0x7ff0000000000000,
            0xfff8000000000042,
            0x7ff8000000001234,
        ];
        let source = input_bits.map(f64::from_bits);
        let array = DenseDouble2d::try_from_col_major(
            [2, 4],
            &source,
            LimitsV1 {
                max_dimension: 4,
                max_elements: 8,
                max_bytes: 64,
            },
        )
        .unwrap();
        let pointer = array.as_slice().as_ptr();
        let array = array.into_negated();
        assert_eq!(array.as_slice().as_ptr(), pointer);
        assert_eq!(array.shape(), [2, 4]);
        for (element, expected) in array.as_slice().iter().zip(expected) {
            assert_eq!(element.to_bits(), expected);
        }
        let restored = array.into_negated();
        assert_eq!(restored.as_slice().as_ptr(), pointer);
        for (element, original) in restored.as_slice().iter().zip(input_bits) {
            assert_eq!(element.to_bits(), original);
        }
    }

    #[test]
    fn huge_empty_extents_do_not_control_a_negation_loop() {
        let limits = LimitsV1 {
            max_dimension: u64::MAX,
            max_elements: 0,
            max_bytes: 0,
        };
        for shape in [[0, 0], [0, u64::MAX], [u64::MAX, 0]] {
            let array = DenseDouble2d::try_from_col_major(shape, &[], limits).unwrap();
            let pointer = array.as_slice().as_ptr();
            let array = array.into_negated();
            assert_eq!(array.shape(), shape);
            assert!(array.as_slice().is_empty());
            assert_eq!(array.as_slice().as_ptr(), pointer);
        }
    }
}
