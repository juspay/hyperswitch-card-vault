use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tartarus::crypto::{aes, jw, Encryption};

const ITERATION: u32 = 14;
const JWE_PRIVATE_KEY: &'static str = include_str!("bench-private-key.pem");
const JWE_PUBLIC_KEY: &'static str = include_str!("bench-public-key.pem");

criterion_main!(benches);
criterion_group!(benches, criterion_aes, criterion_jwe_jws);

pub fn criterion_aes(c: &mut Criterion) {
    let key = aes::generate_aes256_key();
    let algo = aes::GcmAes256::new(key.to_vec());

    {
        let mut group = c.benchmark_group("aes-encryption");
        (1..ITERATION).for_each(|po| {
            let max: u64 = (2_u64).pow(po);
            let value = (0..max).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
            let encrypted_value = algo.encrypt(value.clone()).unwrap();
            group.throughput(criterion::Throughput::Bytes(max));
            group.bench_with_input(
                criterion::BenchmarkId::from_parameter(max),
                &(value, encrypted_value),
                |b, (value, encrypted_value)| {
                    b.iter(|| {
                        black_box(
                            &algo.encrypt(black_box(value.clone())).unwrap() == encrypted_value,
                        )
                    })
                },
            );
        });
    }

    let mut group_2 = c.benchmark_group("aes-decryption");
    (1..ITERATION).for_each(|po| {
        let max: u64 = (2_u64).pow(po);
        let value = (0..max).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
        let encrypted_value = algo.encrypt(value.clone()).unwrap();
        group_2.throughput(criterion::Throughput::Bytes(max));
        group_2.bench_with_input(
            criterion::BenchmarkId::from_parameter(max),
            &(value, encrypted_value),
            |b, (value, encrypted_value)| {
                b.iter(|| {
                    black_box(&algo.encrypt(black_box(encrypted_value.clone())).unwrap() == value)
                })
            },
        );
    });
}

pub fn criterion_jwe_jws(c: &mut Criterion) {
    let algo = jw::JWEncryption::new(JWE_PUBLIC_KEY.to_string(), JWE_PRIVATE_KEY.to_string());

    {
        let mut group = c.benchmark_group("jw-encryption");
        (1..ITERATION).for_each(|po| {
            let max: u64 = (2_u64).pow(po);
            let value = (0..max).map(|_| rand::random::<char>()).collect::<String>();
            let value = value.as_bytes().to_vec();
            let encrypted_value = algo.encrypt(value.clone()).unwrap();
            group.throughput(criterion::Throughput::Bytes(max));
            group.bench_with_input(
                criterion::BenchmarkId::from_parameter(max),
                &(value, encrypted_value),
                |b, (value, encrypted_value)| {
                    b.iter(|| {
                        black_box(
                            &algo.encrypt(black_box(value.clone())).unwrap() == encrypted_value,
                        )
                    })
                },
            );
        });
    }

    let mut group_2 = c.benchmark_group("jw-decryption");
    (1..ITERATION).for_each(|po| {
        let max: u64 = (2_u64).pow(po);
        let value = (0..max).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
        let encrypted_value = algo.encrypt(value.clone()).unwrap();
        group_2.throughput(criterion::Throughput::Bytes(max));
        group_2.bench_with_input(
            criterion::BenchmarkId::from_parameter(max),
            &(value, encrypted_value),
            |b, (value, encrypted_value)| {
                b.iter(|| {
                    black_box(&algo.encrypt(black_box(encrypted_value.clone())).unwrap() == value)
                })
            },
        );
    });
}
