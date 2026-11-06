// SPDX-License-Identifier: GPL-3.0-or-later
use mathlab_array::{ArrayErrorV1, DenseDouble2d, LimitsV1, MtimesErrorV1};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramValueClassV1 {
    Double,
    Logical,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramValueErrorV1 {
    Array(ArrayErrorV1),
    Multiply(MtimesErrorV1),
    Shape,
    Type,
    UnsupportedFeature,
    Allocation,
    Cancelled,
}
impl ProgramValueErrorV1 {
    pub fn is_allocation(&self) -> bool {
        match self {
            Self::Allocation => true,
            Self::Array(a) => a.code.as_str() == "AllocationFailure",
            Self::Multiply(mathlab_array::MtimesErrorV1::Array(a)) => {
                a.code.as_str() == "AllocationFailure"
            }
            _ => false,
        }
    }
}
impl From<ArrayErrorV1> for ProgramValueErrorV1 {
    fn from(e: ArrayErrorV1) -> Self {
        Self::Array(e)
    }
}
pub(super) const LIMITS: LimitsV1 = LimitsV1 {
    max_dimension: 16,
    max_elements: 256,
    max_bytes: 2048,
};
#[derive(Debug)]
pub(super) enum Storage {
    Double(DenseDouble2d),
    Logical { shape: [u64; 2], data: Vec<u8> },
}
#[derive(Debug)]
pub struct ProgramValueV1 {
    pub(super) storage: Storage,
}
impl ProgramValueV1 {
    pub fn try_double(shape: [u64; 2], data: &[f64]) -> Result<Self, ProgramValueErrorV1> {
        Ok(Self {
            storage: Storage::Double(DenseDouble2d::try_from_col_major(shape, data, LIMITS)?),
        })
    }
    pub fn try_logical(shape: [u64; 2], data: &[u8]) -> Result<Self, ProgramValueErrorV1> {
        let n = layout(shape)?;
        if data.len() != n || data.iter().any(|v| *v > 1) {
            return Err(ProgramValueErrorV1::Shape);
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(n)
            .map_err(|_| ProgramValueErrorV1::Allocation)?;
        if owned.capacity() > n {
            return Err(ProgramValueErrorV1::Allocation);
        }
        owned.extend_from_slice(data);
        Ok(Self {
            storage: Storage::Logical { shape, data: owned },
        })
    }
    pub fn try_copy(&self) -> Result<Self, ProgramValueErrorV1> {
        match &self.storage {
            Storage::Double(a) => Self::try_double(a.shape(), a.as_slice()),
            Storage::Logical { shape, data } => Self::try_logical(*shape, data),
        }
    }
    pub fn shape(&self) -> [u64; 2] {
        match &self.storage {
            Storage::Double(a) => a.shape(),
            Storage::Logical { shape, .. } => *shape,
        }
    }
    pub fn class(&self) -> ProgramValueClassV1 {
        match &self.storage {
            Storage::Double(_) => ProgramValueClassV1::Double,
            Storage::Logical { .. } => ProgramValueClassV1::Logical,
        }
    }
    pub fn doubles(&self) -> Option<&[f64]> {
        match &self.storage {
            Storage::Double(a) => Some(a.as_slice()),
            _ => None,
        }
    }
    pub fn logicals(&self) -> Option<&[u8]> {
        match &self.storage {
            Storage::Logical { data, .. } => Some(data),
            _ => None,
        }
    }
    pub fn len(&self) -> usize {
        let s = self.shape();
        (s[0] * s[1]) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn payload_bytes(&self) -> usize {
        self.len()
            * if self.class() == ProgramValueClassV1::Double {
                8
            } else {
                1
            }
    }
    pub fn word(&self, i: usize) -> Option<f64> {
        match &self.storage {
            Storage::Double(a) => a.as_slice().get(i).copied(),
            Storage::Logical { data, .. } => data.get(i).map(|v| *v as f64),
        }
    }
    pub fn scalar(&self) -> Result<f64, ProgramValueErrorV1> {
        if self.shape() != [1, 1] {
            return Err(ProgramValueErrorV1::Shape);
        }
        Ok(self.word(0).unwrap())
    }
    pub fn truth(&self) -> bool {
        !self.is_empty() && (0..self.len()).all(|i| self.word(i).unwrap() != 0.0)
    }
}
pub(super) fn layout(shape: [u64; 2]) -> Result<usize, ProgramValueErrorV1> {
    if shape[0] > 16 || shape[1] > 16 {
        return Err(ProgramValueErrorV1::Shape);
    }
    Ok((shape[0] * shape[1]) as usize)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_allocation_causes_remain_classifiable() {
        let cause = ArrayErrorV1 {
            context: mathlab_array::ErrorContextV1::Construct,
            code: mathlab_array::ErrorCodeV1::AllocationFailure {
                requested_elements: 256,
                requested_bytes: 2048,
            },
        };
        assert!(ProgramValueErrorV1::Array(cause).is_allocation());
        assert!(ProgramValueErrorV1::Multiply(MtimesErrorV1::Array(cause)).is_allocation());
        assert!(ProgramValueErrorV1::Allocation.is_allocation());
        assert!(!ProgramValueErrorV1::Shape.is_allocation());
    }
}
