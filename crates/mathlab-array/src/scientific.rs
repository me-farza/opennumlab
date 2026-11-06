// SPDX-License-Identifier: GPL-3.0-or-later
//! Scientific profile storage/kernels. Does not widen DenseDouble2d or LimitsV1.
mod complex;
mod dense;
mod solve;
pub use complex::Complex64V1;
pub use dense::{ScientificClassV1, ScientificDenseV1, ScientificShapeV1, ScientificReadRangeV1};
pub use solve::{ScientificSolveV1, SolveProgressV1, ScientificSolveDiagnosticsV1};
pub const MAX_RANK_V1:usize=8;
pub const MAX_ELEMENTS_V1:usize=1<<20;
pub const MAX_VALUE_BYTES_V1:usize=16<<20;
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum ScientificErrorV1 { Allocation, Resource, Rank, Shape, Type, Index, Unsupported, Singular, NonFiniteInput, ArithmeticBreakdown, Cancelled }
/// Every actual Vec reserve passes this admission/fault hook immediately before
/// try_reserve_exact. Successful vectors must expose exactly the admitted
/// capacity; allocator overhead/global OOM are not an RSS guarantee.
pub trait ScientificControlV1 {
    fn charge(&mut self,scalar_units:u64)->Result<(),ScientificErrorV1>;
    fn reserve(&mut self,bytes:usize)->Result<(),ScientificErrorV1>;
    /// Preparation suspends BEFORE each bounded CPU chunk, including copies,
    /// zero fill and finite validation. No heap allocation or thread is used.
    async fn checkpoint(&mut self,units:u64)->Result<(),ScientificErrorV1>{
        if units>128{return Err(ScientificErrorV1::Resource)}
        yield_scientific_v1().await;self.charge(units)
    }
}
pub(crate) fn reserve<T>(n:usize,control:&mut impl ScientificControlV1)->Result<Vec<T>,ScientificErrorV1> {
    let bytes=n.checked_mul(std::mem::size_of::<T>()).ok_or(ScientificErrorV1::Resource)?;
    if bytes>MAX_VALUE_BYTES_V1 {return Err(ScientificErrorV1::Resource)}
    let mut v=Vec::new();control.reserve(bytes)?;v.try_reserve_exact(n).map_err(|_|ScientificErrorV1::Allocation)?;
    if v.capacity()!=n {return Err(ScientificErrorV1::Resource)} Ok(v)
}

/// One allocation-free cooperative suspension. Wakes an external executor;
/// synchronous compatibility wrappers drive the same future on the caller stack.
pub async fn yield_scientific_v1(){ScientificYieldV1(true).await}
struct ScientificYieldV1(bool);
impl std::future::Future for ScientificYieldV1{
    type Output=();
    fn poll(self:std::pin::Pin<&mut Self>,cx:&mut std::task::Context<'_>)->std::task::Poll<()>{
        let state=self.get_mut();if state.0{state.0=false;cx.waker().wake_by_ref();std::task::Poll::Pending}else{std::task::Poll::Ready(())}
    }
}
pub(crate) fn drive<F:std::future::Future>(future:F)->F::Output{
    let mut future=std::pin::pin!(future);let mut cx=std::task::Context::from_waker(std::task::Waker::noop());
    loop{if let std::task::Poll::Ready(value)=future.as_mut().poll(&mut cx){return value}}
}

mod sparse;
mod sparse_triplet;
mod sparse_kernels;
pub use sparse::{ScientificCscV1,ScientificCscShapeV1,ScientificCscValuesV1};
pub use sparse_triplet::ScientificTripletValuesV1;

mod qr;
pub use qr::{ScientificQrV1,ScientificQrDiagnosticsV1,ScientificQrPhaseV1};

#[cfg(onl_qr_phase_probe)]
pub use qr::{install_scientific_qr_phase_probe_v1,ScientificQrPhaseProbeGuardV1};
