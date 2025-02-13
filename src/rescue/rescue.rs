use crate::common::{matrix::mmul_assign, sbox::sbox};
use crate::hash::{generic_hash, generic_hash_var_length};
use crate::traits::{HashFamily, HashParams};
use franklin_crypto::bellman::{Engine, Field};
use std::convert::TryInto;

/// Receives inputs whose length `known` prior(fixed-length).
/// Also uses custom domain strategy which basically sets value of capacity element to
/// length of input and applies a padding rule which makes input size equals to multiple of
/// rate parameter.
/// Uses pre-defined state-width=3 and rate=2.
pub fn rescue_hash<E: Engine, const L: usize>(input: &[E::Fr; L]) -> [E::Fr; 2] {
    const STATE_WIDTH: usize = 3;
    const RATE: usize = 2;
    let params = RescueParams::<E, STATE_WIDTH, RATE>::default();
    generic_hash(&params, input)
}

/// Receives inputs whose length `unknown` prior (variable-length).
/// Also uses custom domain strategy which does not touch to value of capacity element
/// and does not apply any padding rule. 
/// Uses pre-defined state-width=3 and rate=2.
pub fn rescue_hash_var_length<E: Engine>(input: &[E::Fr]) -> [E::Fr; 2] {
    // TODO: try to implement const_generics_defaults: https://github.com/rust-lang/rust/issues/44580
    const STATE_WIDTH: usize = 3;
    const RATE: usize = 2;
    let params = RescueParams::<E, STATE_WIDTH, RATE>::default();
    generic_hash_var_length(&params, input)
}

pub fn generic_rescue_hash<
    E: Engine,
    const STATE_WIDTH: usize,
    const RATE: usize,
    const LENGTH: usize,
>(
    input: &[E::Fr; LENGTH],
) -> [E::Fr; RATE] {
    let params = RescueParams::<E, STATE_WIDTH, RATE>::default();
    generic_hash(&params, input)
}

pub fn generic_rescue_var_length<E: Engine, const STATE_WIDTH: usize, const RATE: usize>(
    input: &[E::Fr],
) -> [E::Fr; RATE] {
    let params = RescueParams::<E, STATE_WIDTH, RATE>::default();
    generic_hash_var_length(&params, input)
}
#[derive(Clone, Debug)]
pub struct RescueParams<E: Engine, const STATE_WIDTH: usize, const RATE: usize> {
    pub full_rounds: usize,
    pub round_constants: Vec<[E::Fr; STATE_WIDTH]>,
    pub mds_matrix: [[E::Fr; STATE_WIDTH]; STATE_WIDTH],
    pub alpha: E::Fr,
    pub alpha_inv: E::Fr,
}

impl<E: Engine, const STATE_WIDTH: usize, const RATE: usize> Default
    for RescueParams<E, STATE_WIDTH, RATE>
{
    fn default() -> Self {
        let (params, alpha, alpha_inv) = super::params::rescue_params::<E, STATE_WIDTH, RATE>();
        Self {
            full_rounds: params.full_rounds,
            round_constants: params
                .round_constants()
                .try_into()
                .expect("round constants"),
            mds_matrix: *params.mds_matrix(),
            alpha,
            alpha_inv,
        }
    }
}

impl<E: Engine, const STATE_WIDTH: usize, const RATE: usize> HashParams<E, STATE_WIDTH, RATE>
    for RescueParams<E, STATE_WIDTH, RATE>
{
    fn hash_family(&self) -> HashFamily {
        HashFamily::Rescue
    }

    fn constants_of_round(&self, round: usize) -> [E::Fr; STATE_WIDTH] {
        self.round_constants[round]
    }

    fn mds_matrix(&self) -> [[E::Fr; STATE_WIDTH]; STATE_WIDTH] {
        self.mds_matrix
    }

    fn number_of_full_rounds(&self) -> usize {
        self.full_rounds
    }

    fn number_of_partial_rounds(&self) -> usize {
        unimplemented!("Rescue doesn't have partial rounds.")
    }

    fn alpha(&self) -> E::Fr {
        self.alpha
    }

    fn alpha_inv(&self) -> E::Fr {
        self.alpha_inv
    }

    fn optimized_mds_matrixes(&self) -> (&[[E::Fr; STATE_WIDTH]; STATE_WIDTH], &[[[E::Fr; STATE_WIDTH];STATE_WIDTH]]) {
        unimplemented!("Rescue doesn't use optimized matrixes")
    }

    fn optimized_round_constants(&self) -> &[[E::Fr; STATE_WIDTH]] {
        unimplemented!("Rescue doesn't use optimized round constants")
    }
}

pub(crate) fn rescue_round_function<
    E: Engine,
    P: HashParams<E, STATE_WIDTH, RATE>,
    const STATE_WIDTH: usize,
    const RATE: usize,
>(
    params: &P,
    state: &mut [E::Fr; STATE_WIDTH],
) {
    assert_eq!(params.hash_family(), HashFamily::Rescue, "Incorrect hash family!");
    // round constants for first step
    state
        .iter_mut()
        .zip(params.constants_of_round(0).iter())
        .for_each(|(s, c)| s.add_assign(c));

    for round in 0..2 * params.number_of_full_rounds() {
        // sbox
        if round & 1 == 0 {
            sbox::<E>(params.alpha_inv(), state);
        } else {
            sbox::<E>(params.alpha(), state);
        }

        // mds
        mmul_assign::<E, STATE_WIDTH>(&params.mds_matrix(), state);

        // round constants
        state
            .iter_mut()
            .zip(params.constants_of_round(round + 1).iter())
            .for_each(|(s, c)| s.add_assign(c));
    }
}
