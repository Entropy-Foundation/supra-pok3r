//adapted from https://github.com/arkworks-rs/poly-commit/blob/master/src/kzg10/mod.rs
#![allow(dead_code)]

use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup};
use ark_ec::{scalar_mul::fixed_base::FixedBase, VariableBaseMSM};
use ark_ff::{One, PrimeField, UniformRand, Zero};
use ark_poly::DenseUVPolynomial;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::{
    marker::PhantomData,
    ops::{Div, Mul, Sub},
    vec,
};

use ark_std::rand::RngCore;

pub struct KZG10<E: Pairing, P: DenseUVPolynomial<E::ScalarField>> {
    _engine: PhantomData<E>,
    _poly: PhantomData<P>,
}

#[derive(CanonicalDeserialize, CanonicalSerialize, PartialEq, Debug, Clone)]
pub struct UniversalParams<E: Pairing> {
    /// Group elements of the form `{ \beta^i G }`, where `i` ranges from 0 to `degree`.
    pub powers_of_g: Vec<E::G1Affine>,
    /// Group elements of the form `{ \beta^i H }`, where `i` ranges from 0 to `degree`.
    pub powers_of_h: Vec<E::G2Affine>,
}

impl<E, P> KZG10<E, P>
where
    E: Pairing,
    P: DenseUVPolynomial<E::ScalarField, Point = E::ScalarField>,
    for<'a, 'b> &'a P: Div<&'b P, Output = P>,
    for<'a, 'b> &'a P: Sub<&'b P, Output = P>,
{
    pub fn setup<R: RngCore>(max_degree: usize, rng: &mut R) -> UniversalParams<E> {
        let beta = E::ScalarField::rand(rng);
        let g = E::G1::rand(rng);
        let h = E::G2::rand(rng);

        let mut powers_of_beta = vec![E::ScalarField::one()];

        let mut cur = beta;
        for _ in 0..max_degree {
            powers_of_beta.push(cur);
            cur *= &beta;
        }

        let window_size = FixedBase::get_mul_window_size(max_degree + 1);
        let scalar_bits = E::ScalarField::MODULUS_BIT_SIZE as usize;

        let g_table = FixedBase::get_window_table(scalar_bits, window_size, g);
        let powers_of_g =
            FixedBase::msm::<E::G1>(scalar_bits, window_size, &g_table, &powers_of_beta);

        let h_table = FixedBase::get_window_table(scalar_bits, window_size, h);
        let powers_of_h =
            FixedBase::msm::<E::G2>(scalar_bits, window_size, &h_table, &powers_of_beta);

        let powers_of_g = E::G1::normalize_batch(&powers_of_g);
        let powers_of_h = E::G2::normalize_batch(&powers_of_h);

        UniversalParams {
            powers_of_g,
            powers_of_h,
        }
    }

    pub fn verify_opening_proof(
        params: &UniversalParams<E>,
        comm: &E::G1Affine,
        point: &E::ScalarField,
        value: &E::ScalarField,
        proof: &E::G1Affine,
    ) -> bool {
        let g = params.powers_of_g[0];
        let h = params.powers_of_h[0];
        let beta_h = params.powers_of_h[1];

        let inner = comm.into_group() - &g.mul(value);
        let lhs = E::pairing(inner, h);

        let inner = beta_h.into_group() - &h.mul(point);
        let rhs = E::pairing(proof, inner);

        lhs == rhs
    }

    pub fn commit_g1(params: &UniversalParams<E>, polynomial: &P) -> E::G1Affine {
        let d = polynomial.degree();

        let plain_coeffs: Vec<<<E as Pairing>::ScalarField as PrimeField>::BigInt> =
            convert_to_bigints(polynomial.coeffs());

        let powers_of_g = &params.powers_of_g[..=d].to_vec();
        let commitment =
            <E::G1 as VariableBaseMSM>::msm_bigint(&powers_of_g[..], plain_coeffs.as_slice());
        commitment.into_affine()
    }

    pub fn commit_g2(params: &UniversalParams<E>, polynomial: &P) -> E::G2Affine {
        let d = polynomial.degree();

        let plain_coeffs: Vec<<<E as Pairing>::ScalarField as PrimeField>::BigInt> =
            convert_to_bigints(polynomial.coeffs());

        let powers_of_h = &params.powers_of_h[..=d].to_vec();
        let commitment =
            <E::G2 as VariableBaseMSM>::msm_bigint(&powers_of_h[..], plain_coeffs.as_slice());

        commitment.into_affine()
    }

    pub fn compute_opening_proof(
        params: &UniversalParams<E>,
        polynomial: &P,
        point: &E::ScalarField,
    ) -> E::G1Affine {
        let eval = polynomial.evaluate(point);
        let eval_as_poly = P::from_coefficients_vec(vec![eval]);
        let numerator = polynomial.clone().sub(&eval_as_poly);
        let divisor =
            P::from_coefficients_vec(vec![E::ScalarField::zero() - point, E::ScalarField::one()]);
        let witness_polynomial = numerator.div(&divisor);

        Self::commit_g1(params, &witness_polynomial)
    }
}

fn skip_leading_zeros_and_convert_to_bigints<F: PrimeField, P: DenseUVPolynomial<F>>(
    p: &P,
) -> (usize, Vec<F::BigInt>) {
    let mut num_leading_zeros = 0;
    while num_leading_zeros < p.coeffs().len() && p.coeffs()[num_leading_zeros].is_zero() {
        num_leading_zeros += 1;
    }
    let coeffs = convert_to_bigints(&p.coeffs()[num_leading_zeros..]);
    (num_leading_zeros, coeffs)
}

fn convert_to_bigints<F: PrimeField>(p: &[F]) -> Vec<F::BigInt> {
    let coeffs = p.iter().map(|s| s.into_bigint()).collect::<Vec<_>>();
    coeffs
}
