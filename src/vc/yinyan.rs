use blake2::Blake2b;
use byteorder::{BigEndian, ByteOrder};
use num_bigint::{BigInt, BigUint, RandBigInt};
use num_traits::cast::FromPrimitive;
use num_traits::identities::One;


use crate::hash::hash_prime;
use crate::proofs;
use crate::traits::*;
use rand::{CryptoRng, Rng};
use std::marker::PhantomData;
// pub struct AccTradeoff {
//     g: BigUint,
//     n: BigUint,
//     root: BigUint,
// }
// impl StaticAccumulator for AccTradeoff {
//     #[inline]
//     fn setup<T, R>(rng: &mut R, int_size_bits: usize) -> Self
//     where
//         T: PrimeGroup,
//         R: CryptoRng + Rng,
//     {
//         unimplemented!()
//     }

//     #[inline]
//     fn mem_wit_create(&self, x: &BigUint) -> BigUint {
//         unimplemented!()
//     }

//     #[inline]
//     fn ver_mem(&self, w: &BigUint, x: &BigUint) -> bool {
//         w.modpow(x, &self.n) == self.root
//     }

//     #[inline]
//     fn state(&self) -> &BigUint {
//         &self.root
//     }
//     #[inline]
//     fn add(&mut self, x: &BigUint) {
//         // assumes x is already a prime
//         self.root = self.root.modpow(x, &self.n);
//     }
// }

type Proof = Vec<BigUint>;

pub struct Commitment {
    pub states: Vec<(BigUint, BigUint)>,
    pub prods: Vec<proofs::PoprodProof>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct YinYanVectorCommitment<'a, A: 'a + UniversalAccumulator + BatchedAccumulator + FromParts>
{
    lambda: usize, // security param
    k: usize,      // word size
    size: usize,   // max words in the vector
    uacc: A,
    accs: Vec<(A, A)>, // lenght of accs must be k
    _a: PhantomData<&'a A>,
    prod_proofs: Vec<proofs::PoprodProof>,
    modulus: BigUint,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub lambda: usize,
    pub k: usize,
    pub n: usize,
    pub size: usize,
}

impl<'a, A: 'a + UniversalAccumulator + BatchedAccumulator + FromParts>
    YinYanVectorCommitment<'a, A>
{
    pub fn reset(&mut self) {
        self.accs = self
            .accs
            .iter()
            .map(|acc| {
                (
                    A::from_parts(self.modulus.clone(), acc.0.g().clone()),
                    A::from_parts(self.modulus.clone(), acc.1.g().clone()),
                )
            })
            .collect();
        self.uacc = A::from_parts(self.modulus.clone(), self.uacc.g().clone());
        self.prod_proofs = vec![];
    }
    pub fn re_specialize(&mut self, size: usize) -> &BigUint {
        self.uacc = A::from_parts(self.modulus.clone(), self.uacc.g().clone());
        self.specialize(size)
    }

    pub fn specialize(&mut self, size: usize) -> &BigUint {
        // TODO: if already specialized skip first part
        for i in 0..size {
            // TODO: eventually do batchadd (check how we do it in commit)
            self.uacc.add(&map_i_to_p_i(i));
        }

        self.uacc.state()
    }
}

impl<'a, A: 'a + UniversalAccumulator + BatchedAccumulator + FromParts> StaticVectorCommitment<'a>
    for YinYanVectorCommitment<'a, A>
{
    type Domain = Vec<bool>; // make sure this is of size k
    type Proof = Proof;
    type BatchProof = Vec<BigUint>;
    type Config = Config;
    type State = Vec<(&'a BigUint, &'a BigUint)>;
    type Commitment = Commitment;

    fn setup<G, R>(rng: &mut R, config: &Self::Config) -> Self
    where
        G: PrimeGroup,
        R: CryptoRng + Rng,
    {
        let (modulus, _) = G::generate_primes(rng, config.n).unwrap();

        let two = BigUint::from_u64(2 as u64).unwrap();

        YinYanVectorCommitment {
            lambda: config.lambda,
            k: config.k,
            size: config.size,
            modulus: modulus.clone(),
            prod_proofs: vec![],
            uacc: A::from_parts(modulus.clone(), rng.gen_biguint(config.n)),
            accs: (0..config.k)
                .map(|_| {
                    let tmp = rng.gen_biguint(config.n);
                    let g = tmp.modpow(&two, &modulus);
                    (
                        A::from_parts(modulus.clone(), g.clone()),
                        A::from_parts(modulus.clone(), g),
                    )
                })
                .collect(),
            _a: PhantomData,
        }
    }

    fn commit(&mut self, m: &[Self::Domain]) -> Self::Commitment {
        // i = 0..m (m number of words)
        for (i, v) in m.iter().enumerate() {
            debug_assert!(v.len() == self.k);

            // p_i
            let prime = map_i_to_p_i(i);

            // j = 0..k (k number of bits in each word)
            // TODO: can be done with batch add!
            for (bit, acc) in v.iter().zip(self.accs.iter_mut()) {
                if *bit {
                    // B_j
                    acc.1.add(&prime);
                } else {
                    // A_j
                    acc.0.add(&prime);
                }
            }
        }

        let g = self.uacc.g();
        let (U_n, u) = (self.uacc.state(), self.uacc.set());


        self.prod_proofs = self
            .accs
            .iter()
            .map(|acc| {
                let g_j = acc.0.g();
                debug_assert!(g_j == acc.1.g());
                let (A_j, a_j) = (acc.0.state(), acc.0.set());
                let (B_j, b_j) = (acc.1.state(), acc.1.set());
                proofs::ni_poprod_prove(
                    g,
                    g_j,
                    A_j,
                    &((B_j * U_n) % &self.modulus),
                    a_j,
                    b_j,
                    u,
                    &self.modulus,
                )
            })
            .collect();

        Self::Commitment {
            states: self
                .state()
                .iter()
                .map(|acc| (acc.0.clone(), acc.1.clone()))
                .collect(),
            prods: self.prod_proofs.clone(),
        }
    }

    fn open(&self, b: &Self::Domain, i: usize) -> Self::Proof {
        let p_i = map_i_to_p_i(i);

        let proof: Proof = b
            .iter()
            .zip(self.accs.iter())
            .map(|(bit, acc)| {
                if *bit {
                    acc.1.mem_wit_create(&p_i)
                } else {
                    acc.0.mem_wit_create(&p_i)
                }
            })
            .collect();

        proof
    }

    fn verify(&self, b: &Self::Domain, i: usize, pi: &Self::Proof) -> bool {
        let p_i = map_i_to_p_i(i);

        // Make sure proof is of the right size
        if self.prod_proofs.len() != self.k || pi.len() != self.k {
            return false;
        }

        // Verify accumulator proof
        let accs_check = b
            .iter()
            .zip(self.accs.iter())
            .zip(pi.iter())
            .all(|((bit, acc), w)| {
                if *bit {
                    acc.1.ver_mem(w, &p_i)
                } else {
                    acc.0.ver_mem(w, &p_i)
                }
            });

        // Verify product proof
        let uacc_check = {
            let g = self.uacc.g();
            let U_n = self.uacc.state();
            self.accs
                .iter()
                .zip(self.prod_proofs.iter())
                .all(|(acc, prod)| {
                    let g_j = acc.0.g();
                    let (A_j, B_j) = (acc.0.state(), acc.1.state());
                    proofs::ni_poprod_verify(
                        g,                              // g
                        g_j,                            // h
                        A_j,                            // y1
                        &((B_j * U_n) % &self.modulus), // y2
                        prod,                           // pi
                        &self.modulus,
                    )
                })
        };

        accs_check && uacc_check
    }

    fn state(&'a self) -> Self::State {
        self.accs
            .iter()
            .map(|acc| (acc.0.state(), acc.1.state()))
            .collect()
    }

    fn batch_open(&self, b: &[Self::Domain], i: &[usize]) -> Self::BatchProof {
        unimplemented!();
    }

    fn batch_verify(&self, b: &[Self::Domain], i: &[usize], pi: &Self::BatchProof) -> bool {
        unimplemented!();
    }
}

// impl<'a, A: 'a + UniversalAccumulator + BatchedAccumulator + FromParts> DynamicVectorCommitment<'a>
//     for YinYanVectorCommitment<'a, A>
// {
//     fn update(&mut self, b: &Self::Domain, b_prime: &Self::Domain, i: usize) {
//         unimplemented!();
//         // if b == b_prime {
//         //     // Nothing to do
//         // } else if *b {
//         //     self.acc.add(&map_i_to_p_i(i));
//         // } else {
//         //     self.acc.del(&map_i_to_p_i(i)).expect("not a member");
//         // }
//     }
// }

fn map_i_to_p_i(i: usize) -> BigUint {
    let mut to_hash = [0u8; 8];
    BigEndian::write_u64(&mut to_hash, i as u64);
    hash_prime::<_, Blake2b>(&to_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accumulator::Accumulator;
    use crate::group::RSAGroup;
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaChaRng;

    fn fake_vector(m: usize, k: usize) -> Vec<Vec<bool>> {
        let mut rng = ChaChaRng::from_seed([0u8; 32]);
        let mut val: Vec<Vec<bool>> = (0..m)
            .map(|_| (0..k).map(|_| rng.gen()).collect())
            .collect();
        // set two bits manually, to make checks easier
        val[2] = vec![true, true];
        val[3] = vec![false, false];

        val
    }

    #[test]
    fn test_yinyan_vc_basics() {

        // Set up vector commitment
        let mut rng = ChaChaRng::from_seed([0u8; 32]);
        let config = Config {
            lambda: 128,
            k: 2,
            n: 1024,
            size: 4,
        };
        let mut vc = YinYanVectorCommitment::<Accumulator>::setup::<RSAGroup, _>(&mut rng, &config);

        // Specialize & commit to a vector
        let val = fake_vector(config.size, config.k);
        vc.specialize(val.len());
        vc.commit(&val);

        // Open a particular index
        let proof = vc.open(&vec![true, true], 2);

        // Accept a correct opening
        assert!(
            vc.verify(&vec![true, true], 2, &proof),
            "invalid commitment (bit set)"
        );

        // Reject an incorrect witness
        assert!(
            !vc.verify(&vec![false, false], 2, &proof),
            "verification should not pass"
        );

        // Reject a correct witness with an incorrect proof
        assert!(
            !vc.verify(&vec![false, false], 3, &proof),
            "verification should not pass"
        );

        // Reject a proof if we have a different specialization
        let mut vc_fake = vc.clone();
        vc_fake.re_specialize(val.len() + 1);
        assert!(
            !vc_fake.verify(&vec![true, true], 2, &proof),
            "union proof should not verify"
        );

        // Accept is re-specialized correctly
        vc_fake.re_specialize(val.len());
        assert!(
            vc_fake.verify(&vec![true, true], 2, &proof),
            "union proof should not verify"
        );

    }
}
