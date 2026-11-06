// SPDX-License-Identifier: GPL-3.0-or-later
//! Original binary64 complex arithmetic; separate products/additions, no FMA.
use super::ScientificErrorV1 as E;
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Complex64V1 { pub re: f64, pub im: f64 }
impl Complex64V1 {
    pub const ZERO: Self = Self { re: 0., im: 0. };
    pub const ONE: Self = Self { re: 1., im: 0. };
    pub fn real(re: f64) -> Self { Self { re, im: 0. } }
    pub fn finite(self) -> bool { self.re.is_finite() && self.im.is_finite() }
    pub fn conj(self) -> Self { Self { re: self.re, im: -self.im } }
    pub fn add(self, b: Self) -> Self { Self { re: self.re+b.re, im: self.im+b.im } }
    pub fn sub(self, b: Self) -> Self { Self { re: self.re-b.re, im: self.im-b.im } }
    pub fn mul(self, b: Self) -> Self {
        let ac=self.re*b.re; let bd=self.im*b.im;
        let ad=self.re*b.im; let bc=self.im*b.re;
        Self { re: ac-bd, im: ad+bc }
    }
    /// First maximum of this component metric selects a pivot. It deliberately
    /// does not claim MATLAB modulus-pivot ordering or universal GEPP stability.
    pub fn pivot_metric(self) -> f64 { self.re.abs().max(self.im.abs()) }
    /// Power-of-two normalization keeps dot/cross intermediates bounded.
    /// Gradual underflow/rounded zero follows binary64; overflow is breakdown.
    /// Cancellation in dot/cross is not claimed correctly rounded.
    pub fn checked_div(self, b: Self) -> Result<Self,E> {
        if !self.finite() || !b.finite() { return Err(E::NonFiniteInput); }
        let db=b.pivot_metric(); if db==0. { return Err(E::Singular); }
        let ed=exponent(db);
        // Only the positive denominator is normalized jointly. The two
        // numerator terms retain INDEPENDENT exponent scales, so division by
        // 1 or i cannot erase a small representable component next to a huge
        // one. Tiny denominator-square terms may round away below its ulp.
        let br=scale(b.re,-ed)?;let bi=scale(b.im,-ed)?;
        let den=br*br+bi*bi;
        Ok(Self {
            re: quotient_sum(self.re,b.re,self.im,b.im,den,-2*ed)?,
            im: quotient_sum(self.im,b.re,-self.re,b.im,den,-2*ed)?,
        })
    }
}
/// floor(log2(abs(x))) for positive finite nonzero x, including subnormals.
fn exponent(x:f64)->i32 {
    let bits=x.to_bits()&0x7fff_ffff_ffff_ffff; let e=(bits>>52) as i32;
    if e!=0 { e-1023 } else { 63-(bits.leading_zeros() as i32)-1074 }
}
/// At most six factors for the extended product/quotient exponent range.
/// Factors keep intermediate magnitudes monotone, so no false intermediate
/// overflow occurs when the final scaling is representable.
fn scale(mut x:f64,mut n:i32)->Result<f64,E> {
    while n>1023 { x*=f64::from_bits((2046u64)<<52); n-=1023; if !x.is_finite(){return Err(E::ArithmeticBreakdown)} }
    while n< -1022 { x*=f64::from_bits(1u64<<52); n+=1022; }
    if n!=0 { x*=f64::from_bits(((n+1023) as u64)<<52); }
    if x.is_finite() {Ok(x)} else {Err(E::ArithmeticBreakdown)}
}

#[derive(Clone,Copy)]
struct Product { mantissa:f64, exponent:i32 }
fn product(a:f64,b:f64)->Result<Product,E>{
    if a==0.||b==0.{return Ok(Product{mantissa:a*b,exponent:0})}
    let ea=exponent(a.abs());let eb=exponent(b.abs());
    Ok(Product{mantissa:scale(a,-ea)?*scale(b,-eb)?,exponent:ea+eb})
}
fn quotient_sum(a:f64,b:f64,c:f64,d:f64,den:f64,shift:i32)->Result<f64,E>{
    let x=product(a,b)?;let y=product(c,d)?;
    if x.mantissa==0.{return scale(y.mantissa/den,y.exponent+shift)}
    if y.mantissa==0.{return scale(x.mantissa/den,x.exponent+shift)}
    let common=x.exponent.max(y.exponent);
    let sum=scale(x.mantissa,x.exponent-common)?+scale(y.mantissa,y.exponent-common)?;
    scale(sum/den,common+shift)
}
