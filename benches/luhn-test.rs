use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tartarus::validations::luhn_on_string;

fn card_number_generator() -> String {
    (0..16)
        .map(|_| format!("{}", rand::random::<u8>() % 10))
        .collect()
}

pub fn criterion_luhn(c: &mut Criterion) {
    c.bench_function("card-number-generator", |b| {
        b.iter(|| black_box(card_number_generator()))
    });
    c.bench_function("luhn-validation", |b| {
        b.iter(|| black_box(luhn_on_string(&black_box(card_number_generator()))))
    });
    let card_number = card_number_generator();
    c.bench_function("luhn", |b| {
        b.iter(|| black_box(luhn_on_string(black_box(&card_number))))
    });
}

criterion_group!(benches, criterion_luhn);
criterion_main!(benches);
