#[cfg(feature = "bls12_377")]
use ark_bls12_377::g1;
#[cfg(feature = "bls12_381")]
use ark_bls12_381::g1;

use crate::common::G1;
use ark_crypto_primitives::crh::sha256::Sha256;
use ark_ec::hashing::{
    curve_maps::wb::WBMap, map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve,
};
use ark_ff::field_hashers::DefaultFieldHasher;

pub type FrHasher = DefaultFieldHasher<Sha256>;
pub type G1Hasher = MapToCurveBasedHasher<G1, FrHasher, WBMap<g1::Config>>;

#[cfg(feature = "bls12_377")]
pub const DOMAIN_STRING_HASH_ID: &'static [u8] =
    b"SUPRA_POKER_ID-hashtoG1-with-BLS12377G1_XMD:SHA-256_SSWU_RO";
#[cfg(feature = "bls12_381")]
pub const DOMAIN_STRING_HASH_ID: &'static [u8] =
    b"SUPRA_POKER_ID-hashtoG1-with-BLS12381G1_XMD:SHA-256_SSWU_RO";

pub fn hash_to_g1(inp: &[u8]) -> G1 {
    hash_to_g1_domain(DOMAIN_STRING_HASH_ID, inp)
}

pub fn hash_to_g1_domain(dom: &[u8], inp: &[u8]) -> G1 {
    let hasher = <G1Hasher as HashToCurve<G1>>::new(dom).expect("failed to create hasher");
    hasher.hash(inp).expect("failed to hash").into()
}
