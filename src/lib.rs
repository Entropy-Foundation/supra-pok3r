pub mod address_book;
pub mod common;
pub mod encoding;
pub mod evaluator;
pub mod hash;
pub mod kzg;
pub mod network;
pub mod shamir;
pub mod shuffler;
pub mod utils;

#[cfg(not(any(feature = "bls12_381", feature = "bls12_377")))]
compile_error!("Enable exactly one curve feature: `bls12_381` or `bls12_377`.");

#[cfg(all(feature = "bls12_381", feature = "bls12_377"))]
compile_error!("`bls12_381` and `bls12_377` are mututally exclusive features.");
