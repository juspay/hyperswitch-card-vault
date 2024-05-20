use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tartarus::validations::{luhn, MAX_CARD_NUMBER_LENGTH};

#[allow(clippy::expect_used)]
fn card_number_generator() -> Vec<u8> {
    (0..16).fold(Vec::with_capacity(MAX_CARD_NUMBER_LENGTH), |mut acc, _| {
        acc.push(rand::random::<u8>() % 10);
        acc
    })
}

pub fn criterion_luhn(c: &mut Criterion) {
    c.bench_function("card-number-generator", |b| {
        b.iter(|| black_box(card_number_generator()))
    });
    c.bench_function("luhn-validation", |b| {
        b.iter(|| black_box(luhn(&black_box(card_number_generator()))))
    });
    let card_number = card_number_generator();
    c.bench_function("luhn", |b| {
        b.iter(|| black_box(luhn(black_box(&card_number))))
    });
}

criterion_group!(benches, criterion_luhn);
criterion_main!(benches);
