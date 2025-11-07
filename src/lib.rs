#![no_std]

#[cfg(feature = "bnum")]
#[path = "lib_bnum.rs"]
mod implementation;

#[cfg(all(not(feature = "bnum"), feature = "fixed"))]
#[path = "lib_fixed.rs"]
mod implementation;

#[cfg(all(not(feature = "bnum"), not(feature = "fixed"), feature = "plain"))]
#[path = "lib_plain.rs"]
mod implementation;

#[cfg(all(not(feature = "bnum"), not(feature = "fixed"), not(feature = "plain")))]
#[path = "lib_crypto.rs"]
mod implementation;

pub use implementation::*;
