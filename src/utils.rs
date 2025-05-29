#![allow(dead_code)]

use ark_crypto_primitives::crh::sha256::Sha256;
use ark_ff::field_hashers::{DefaultFieldHasher, HashToField};
use ark_ff::Field;
use ark_poly::{
    univariate::DensePolynomial, EvaluationDomain, Evaluations, GeneralEvaluationDomain,
    Polynomial, Radix2EvaluationDomain,
};
use ark_std::{ops::Sub, UniformRand};

use crate::common::F;

macro_rules! requires_power_of_2 {
    ($x:expr) => {
        assert!($x > 0 && ($x & ($x - 1)) == 0, "{} is not a power of 2", $x);
    };
}

/// returns a generator of the multiplicative subgroup of input size n
pub fn multiplicative_subgroup_of_size(n: u64) -> F {
    requires_power_of_2!(n);
    let domain = Radix2EvaluationDomain::<F>::new(n as usize).unwrap();
    domain.group_gen
}

/// returns lagrange basis polynomial for index i
pub fn compute_lagrange_basis(i: u64, n: u64) -> DensePolynomial<F> {
    let mut evals = vec![];
    for j in 0..n {
        if i == j {
            evals.push(F::from(1));
        } else {
            evals.push(F::from(0));
        }
    }
    self::interpolate_poly_over_mult_subgroup(&evals)
}

/// returns t(X) = X^n - 1
pub fn compute_vanishing_poly(n: usize) -> DensePolynomial<F> {
    let mut coeffs = vec![];
    for i in 0..n + 1 {
        if i == 0 {
            coeffs.push(F::from(0) - F::from(1)); // -1
        } else if i == n {
            coeffs.push(F::from(1)); // X^n
        } else {
            coeffs.push(F::from(0));
        }
    }
    DensePolynomial { coeffs }
}

/// interpolate polynomial which evaluates to points in v
/// the domain is the powers of n-th root of unity, where n is size of v
/// assumes n is a power of 2
pub fn interpolate_poly_over_mult_subgroup(v: &Vec<F>) -> DensePolynomial<F> {
    let n = v.len();
    let mut evals = vec![];
    for i in 0..n {
        evals.push(v[i]);
    }

    let domain = GeneralEvaluationDomain::<F>::new(n).unwrap();
    let eval_form = Evaluations::from_vec_and_domain(evals, domain);
    eval_form.interpolate()
}

pub fn compute_additive_shares(value: &F, num_shares: usize) -> Vec<F> {
    let mut sum = F::from(0);
    let mut shares = vec![];
    for _ in 1..num_shares {
        let r = F::rand(&mut rand::thread_rng());
        //let r_bs58 = bs58::encode(utils::field_to_bytes(&r)).into_string();
        shares.push(r);
        sum += r;
    }
    shares.push(value.sub(&sum));

    shares
}

pub fn compute_root(x: &F) -> F {
    x.sqrt().unwrap()
}

pub fn compute_power(x: &F, n: u64) -> F {
    x.pow([n])
}

pub fn fs_hash(x: Vec<&[u8]>, num_output: usize) -> Vec<F> {
    let hasher = <DefaultFieldHasher<Sha256> as HashToField<F>>::new(b"pok3r");

    hasher.hash_to_field(&x.concat(), num_output)
}

//computes f(x/ω)
pub fn poly_domain_div_ω(f: &DensePolynomial<F>, ω: &F) -> DensePolynomial<F> {
    let mut new_poly = f.clone();
    for i in 1..(f.degree() + 1) {
        //we don't touch the zeroth coefficient
        let ω_pow_i: F = ω.pow([i as u64]);
        new_poly.coeffs[i] /= ω_pow_i;
    }
    new_poly
}

#[cfg(test)]
mod tests {
    use super::multiplicative_subgroup_of_size;
    use crate::common::F;
    use ark_ff::Field;

    #[test]
    fn test_multiplicative_subgroup_of_size() {
        let n: u64 = 64;
        let ω = multiplicative_subgroup_of_size(n);

        //check if ω^n = 1
        let ω_pow_n_minus_1 = ω.pow([n - 1]);
        let ω_pow_n = ω.pow([n]);
        let one = F::from(1);
        assert_eq!(ω_pow_n, ω_pow_n_minus_1 * ω);
        assert_eq!(one, ω_pow_n);

        //test if all other powers of ω are != 1
        for i in 1..n {
            let ω_pow_i = ω.pow([i]);
            let one = F::from(1);
            assert_ne!(ω_pow_i, one);
        }
    }
}
