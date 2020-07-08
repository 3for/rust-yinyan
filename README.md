# Vector Commitments with Incremental Aggregation based on Accumulators

This code is an implementation of some of the constructions in [Vector Commitment Techniques and Applications to Verifiable Decentralized Storage](https://eprint.iacr.org/2020/149) by Campanelli et al..
The accumulator constructions are based on [this paper](https://eprint.iacr.org/2018/1188), and built upon this [repo](https://github.com/dignifiedquire/rust-accumulators).

To solve the `MaybeUninit` problem, use the latest rustc nightly version instead of `nightly-2019-05-12`.
```
rustup default nightly
```



