use crate::kzg::KZG10;
use ark_ec::pairing::{Pairing, PairingOutput};
use ark_poly::univariate::DensePolynomial;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use serde::{Deserialize, Serialize};

pub const PERM_SIZE: usize = 64;
pub const DECK_SIZE: usize = 52;
pub const LOG_PERM_SIZE: usize = 6;
pub const NUM_SAMPLES: usize = 420;
pub const NUM_BEAVER_TRIPLES: usize = 3466;
pub const NUM_RAND_SHARINGS: usize = 987;

#[cfg(feature = "bls12_377")]
pub type Curve = ark_bls12_377::Bls12_377;
#[cfg(feature = "bls12_381")]
pub type Curve = ark_bls12_381::Bls12_381;

pub type F = <Curve as Pairing>::ScalarField;
pub type G1 = <Curve as Pairing>::G1;
pub type G2 = <Curve as Pairing>::G2;
pub type Gt = PairingOutput<Curve>;
pub type KZG = KZG10<Curve, DensePolynomial<F>>;

/// EvalNetMsg represents the types of messages that
/// we expect to flow between the evaluator and networkd
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EvalNetMsg {
    ConnectionEstablished {
        success: bool,
    },
    Greeting {
        message: String,
    },
    PublishValue {
        sender: String,
        handle: String,
        value: String,
    },
    PublishBatchValue {
        sender: String,
        handles: Vec<String>,
        values: Vec<String>,
    },
}

/// PermutationProof is a structure for the permutation proofs
#[derive(Clone, Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct PermutationProof {
    pub y1: F,
    pub y2: F,
    pub y3: F,
    pub y4: F,
    pub y5: F,
    pub pi_1: G1,
    pub pi_2: G1,
    pub pi_3: G1,
    pub pi_4: G1,
    pub pi_5: G1,
    pub f_com: G1,
    pub q_com: G1,
    pub t_com: G1,
}

pub type Ciphertext = (G2, Vec<Gt>);

#[derive(Clone, Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct EncryptionProof {
    pub pk: G2,
    pub ids: Vec<Vec<u8>>,
    pub card_commitment: G1, //same as f_com above
    pub card_poly_eval: F,
    pub eval_proof: G1,
    pub hiding_ciphertext: Gt,
    pub t: Gt,
    pub sigma_proof: Option<SigmaProof>,
}

#[derive(Clone, Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct SigmaProof {
    pub a1: G2,
    pub a2: Gt,
    pub y: F,
}
