use super::hash::{circuit_generic_hash, circuit_generic_hash_var_length};
use super::sbox::sbox_quintic;
use super::utils::{matrix_vector_product, mul_by_sparse_matrix};
use crate::traits::{HashFamily, HashParams};
use crate::poseidon::PoseidonParams;
use franklin_crypto::bellman::plonk::better_better_cs::cs::ConstraintSystem;
use franklin_crypto::bellman::{Field, SynthesisError};
use franklin_crypto::{
    bellman::Engine,
    plonk::circuit::{allocated_num::Num, linear_combination::LinearCombination},
};
use std::convert::TryInto;

/// Receives inputs whose length `known` prior(fixed-length).
/// Also uses custom domain strategy which basically sets value of capacity element to
/// length of input and applies a padding rule which makes input size equals to multiple of
/// rate parameter.
/// Uses pre-defined state-width=3 and rate=2.
pub fn gadget_poseidon_hash<E: Engine, CS: ConstraintSystem<E>, const L: usize>(
    cs: &mut CS,
    input: &[Num<E>; L],
) -> Result<[Num<E>; 2], SynthesisError> {
    const STATE_WIDTH: usize = 3;
    const RATE: usize = 2;
    let params = PoseidonParams::<E, STATE_WIDTH, RATE>::default();
    circuit_generic_hash(cs, &params, input).map(|res| res.try_into().expect(""))
}

/// Receives inputs whose length `unknown` prior (variable-length).
/// Also uses custom domain strategy which does not touch to value of capacity element
/// and does not apply any padding rule.
/// Uses pre-defined state-width=3 and rate=2.
pub fn gadget_rescue_hash_var_length<E: Engine, CS: ConstraintSystem<E>>(
    cs: &mut CS,
    input: &[Num<E>],
) -> Result<[Num<E>; 2], SynthesisError> {
    // TODO: try to implement const_generics_defaults: https://github.com/rust-lang/rust/issues/44580
    const STATE_WIDTH: usize = 3;
    const RATE: usize = 2;
    let params = PoseidonParams::<E, STATE_WIDTH, RATE>::default();
    circuit_generic_hash_var_length(cs, &params, input).map(|res| res.try_into().expect(""))
}

pub fn gadget_generic_rescue_hash<
    E: Engine,
    CS: ConstraintSystem<E>,
    const STATE_WIDTH: usize,
    const RATE: usize,
    const LENGTH: usize,
>(
    cs: &mut CS,
    input: &[Num<E>; LENGTH],
) -> Result<[Num<E>; RATE], SynthesisError> {
    let params = PoseidonParams::<E, STATE_WIDTH, RATE>::default();
    circuit_generic_hash(cs, &params, input).map(|res| res.try_into().expect(""))
}

pub fn gadget_generic_rescue_hash_var_length<
    E: Engine,
    CS: ConstraintSystem<E>,
    const STATE_WIDTH: usize,
    const RATE: usize,
>(
    cs: &mut CS,
    input: &[Num<E>],
) -> Result<[Num<E>; RATE], SynthesisError> {
    let params = PoseidonParams::<E, STATE_WIDTH, RATE>::default();
    circuit_generic_hash_var_length(cs, &params, input).map(|res| res.try_into().expect(""))
}
pub(crate) fn gadget_poseidon_round_function<
    E: Engine,
    CS: ConstraintSystem<E>,
    P: HashParams<E, STATE_WIDTH, RATE>,
    const STATE_WIDTH: usize,
    const RATE: usize,
>(
    cs: &mut CS,
    params: &P,
    state: &mut [LinearCombination<E>; STATE_WIDTH],
) -> Result<(), SynthesisError> {
    assert_eq!(
        params.hash_family(),
        HashFamily::Poseidon,
        "Incorrect hash family!"
    );
    assert!(params.number_of_full_rounds() % 2 == 0);

    let half_of_full_rounds = params.number_of_full_rounds() / 2;

    let (m_prime, sparse_matrixes) = &params.optimized_mds_matrixes();
    let optimized_round_constants = &params.optimized_round_constants();

    // first full rounds
    for round in 0..half_of_full_rounds {
        let round_constants = &optimized_round_constants[round];

        // add round constatnts
        for (s, c) in state.iter_mut().zip(round_constants.iter()) {
            s.add_assign_constant(*c);
        }
        // non linear sbox
        sbox_quintic::<E, _>(cs, state)?;

        // mul state by mds
        *state = matrix_vector_product(cs, &params.mds_matrix(), state)?;
    }

    state
        .iter_mut()
        .zip(optimized_round_constants[half_of_full_rounds].iter())
        .for_each(|(a, b)| a.add_assign_constant(*b));

    *state = matrix_vector_product(cs, &m_prime, state)?;

    let mut constants_for_partial_rounds = optimized_round_constants
        [half_of_full_rounds + 1..half_of_full_rounds + params.number_of_partial_rounds()]
        .to_vec();
    constants_for_partial_rounds.push([E::Fr::zero(); STATE_WIDTH]);
    // in order to reduce gate number we merge two consecutive iteration
    // which costs 2 gates per each
    for (round_constant, sparse_matrix) in constants_for_partial_rounds
        [..constants_for_partial_rounds.len() - 1]
        .chunks(2)
        .zip(sparse_matrixes[..sparse_matrixes.len() - 1].chunks(2))
    {
        // first
        sbox_quintic::<E, _>(cs, &mut state[..1])?;
        state[0].add_assign_constant(round_constant[0][0]);
        *state = mul_by_sparse_matrix(state, &sparse_matrix[0]);

        // second
        sbox_quintic::<E, _>(cs, &mut state[..1])?;
        state[0].add_assign_constant(round_constant[1][0]);
        *state = mul_by_sparse_matrix(state, &sparse_matrix[1]);
        // reduce gate cost: LC -> Num -> LC
        for state in state.iter_mut() {
            let num = state.clone().into_num(cs).expect("a num");
            *state = LinearCombination::from(num.get_variable());
        }
    }

    sbox_quintic::<E, _>(cs, &mut state[..1])?;
    state[0].add_assign_constant(constants_for_partial_rounds.last().unwrap()[0]);
    *state = mul_by_sparse_matrix(state, &sparse_matrixes.last().unwrap());

    // second full round
    for round in (params.number_of_partial_rounds() + half_of_full_rounds)
        ..(params.number_of_partial_rounds() + params.number_of_full_rounds())
    {
        let round_constants = &optimized_round_constants[round];

        // add round constatnts
        for (s, c) in state.iter_mut().zip(round_constants.iter()) {
            s.add_assign_constant(*c);
        }

        sbox_quintic::<E, _>(cs, state)?;

        // mul state by mds
        *state = matrix_vector_product(cs, &params.mds_matrix(), state)?;
    }

    Ok(())
}
