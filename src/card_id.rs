use ark_ff::One;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::Zero;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

use crate::{
    common::{G1, PERM_SIZE},
    hash::{hash_to_g1, hash_to_g1_domain},
};

pub const CARD_ID_SIZE: usize = 9;

/// Identity used for IBE. Each card is incrypted to the identity ([card_no, deck_no])
#[derive(
    Debug,
    Serialize,
    Deserialize,
    Hash,
    Clone,
    PartialEq,
    Eq,
    CanonicalSerialize,
    CanonicalDeserialize,
)]
pub struct CardId {
    pub card_no: u8,
    pub deck_no: u64,
}
pub fn gen_ids(deck_no: u64) -> Vec<CardId> {
    (0..PERM_SIZE as u8)
        .into_iter()
        .map(|card_no| CardId { deck_no, card_no })
        .collect::<Vec<CardId>>()
}

impl PartialOrd for CardId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(BigUint::from(self).cmp(&BigUint::from(other)))
    }
}

impl Ord for CardId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        BigUint::from(self).cmp(&BigUint::from(other))
        //self.into::<BigUint>().cmp(&other.into::<BigUint>())
    }
}

impl CardId {
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 9 {
            return Err(String::from("input not of length 9 to convert to CardId"));
        }
        let card_no = bytes[0];
        if card_no >= 52 {
            return Err(String::from("must have 0 <= card_no <= 51"));
        }
        let deck_no = u64::from_be_bytes(
            bytes[1..]
                .try_into()
                .expect("failed to parse bytes to CardId bad deck_no"),
        );
        Ok(CardId { card_no, deck_no })
    }

    pub fn to_bytes(&self) -> [u8; 9] {
        let mut out = [0; 9];
        out[0] = self.card_no;
        out[1..].copy_from_slice(&self.deck_no.to_be_bytes());
        out
    }

    pub fn hash_to_g1(&self) -> G1 {
        //hash_to_g1(&BigUint::from(self).to_bytes_le())
        hash_to_g1(&self.to_bytes())
    }
    pub fn hash_to_g1_domain(&self, dom: &[u8]) -> G1 {
        //hash_to_g1_domain(dom, &BigUint::from(self).to_bytes_le())
        hash_to_g1_domain(dom, &self.to_bytes())
    }
}

impl TryFrom<BigUint> for CardId {
    type Error = String;

    fn try_from(value: BigUint) -> Result<Self, Self::Error> {
        CardId::try_from(&value)
    }
}
impl TryFrom<&BigUint> for CardId {
    type Error = String;

    // TODO do all this without clones
    fn try_from(value: &BigUint) -> Result<Self, Self::Error> {
        let len_bits = CARD_ID_SIZE * 8;
        //let one = BigUint::one();
        let card_no_flags: BigUint = (BigUint::one() << 8) - BigUint::one();
        let deck_no_flags = (BigUint::one() << len_bits) - BigUint::one() - card_no_flags.clone();

        println!("card_no_flags = {:?}", card_no_flags.clone().to_bytes_be());
        println!("deck_no_flags = {:?}", deck_no_flags.clone().to_bytes_be());

        // check if functioning as intended
        assert!(
            deck_no_flags.clone() | card_no_flags.clone()
                == (BigUint::one() << len_bits) - BigUint::one(),
            "invalid bitwise or"
        );
        assert!(
            deck_no_flags.clone() & card_no_flags.clone()
                == (BigUint::one() << len_bits) - BigUint::one(),
            "invalid bitwise and"
        );

        // check that input is actually correct length
        let is_zero: BigUint = value.clone() & ((BigUint::one() << len_bits) - BigUint::one());
        assert!(is_zero.is_zero(), "value too large");

        let card_no = (value & card_no_flags)
            .try_into()
            .expect("failed to convert BigUint to u8");
        let deck_no_big: BigUint = (value & deck_no_flags) >> 8;
        let deck_no = deck_no_big
            .try_into()
            .expect("failed to convert BigUint to u64");

        Ok(Self { card_no, deck_no })
    }
}

impl From<&CardId> for BigUint {
    fn from(value: &CardId) -> Self {
        let deck_no_big = BigUint::from(value.deck_no);
        let card_no_big = BigUint::from(value.card_no);
        (deck_no_big << 8) + card_no_big
    }
}
impl From<CardId> for BigUint {
    fn from(value: CardId) -> Self {
        BigUint::from(&value)
    }
}
