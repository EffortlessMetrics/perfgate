use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use perfgate_sha256::sha256_hex;

fn bench_small_inputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_inputs");

    let sizes: Vec<usize> = vec![1, 10, 50, 100];
    for size in sizes {
        let data: Vec<u8> = (0..size as u8).cycle().take(size).collect();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("bytes", size), &data, |b, data| {
            b.iter(|| sha256_hex(black_box(data)));
        });
    }
    group.finish();
}

fn bench_medium_inputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("medium_inputs");

    let sizes: Vec<usize> = vec![1024, 4 * 1024, 10 * 1024];
    for size in sizes {
        let data: Vec<u8> = (0..=255u8).cycle().take(size).collect();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::new("bytes", size), &data, |b, data| {
            b.iter(|| sha256_hex(black_box(data)));
        });
    }
    group.finish();
}

fn bench_large_inputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_inputs");
    group.sample_size(10);

    let size: usize = 1024 * 1024;
    let data: Vec<u8> = (0..=255u8).cycle().take(size).collect();
    group.throughput(Throughput::Bytes(size as u64));
    group.bench_with_input(BenchmarkId::new("bytes", size), &data, |b, data| {
        b.iter(|| sha256_hex(black_box(data)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_small_inputs,
    bench_medium_inputs,
    bench_large_inputs
);
criterion_main!(benches);
