#![allow(clippy::expect_used)]
#![allow(clippy::missing_panics_doc)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tartarus::crypto::hash_manager::{hash_interface::Encode, managers::sha::HmacSha512};

const ITERATION: u32 = 14;

criterion_main!(benches);
criterion_group!(benches, criterion_hmac_sha512);

macro_rules! const_iter {
    ($algo:ident, $body:block, $key:ident, $($ty:expr),*) => {
        $(
            let $algo = $ty($key.clone());
            $body
        )*
    };
}

pub fn criterion_hmac_sha512(c: &mut Criterion) {
    let key: masking::Secret<_> = (0..1000)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<_>>()
        .into();

    const_iter!(
        algo,
        {
            let mut group = c.benchmark_group(format!("{}", algo));
            (1..ITERATION).for_each(|po| {
                let max: u64 = (2_u64).pow(po);
                let value = (0..max).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
                let hashed = algo
                    .encode(value.clone().into())
                    .expect("Failed while hashing");
                group.throughput(criterion::Throughput::Bytes(max));
                group.bench_with_input(
                    criterion::BenchmarkId::from_parameter(format!("{}", max)),
                    &value,
                    |b, value| {
                        b.iter(|| {
                            black_box(
                                algo.encode(black_box(value.clone().into()))
                                    .expect("Failed while hashing")
                                    == hashed,
                            )
                        })
                    },
                );
            })
        },
        key,
        HmacSha512::<1>::new,
        HmacSha512::<10>::new,
        HmacSha512::<100>::new,
        HmacSha512::<1000>::new
    );
}
