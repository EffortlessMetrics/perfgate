use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use perfgate_stats::{percentile, summarize_f64, summarize_u64};

fn bench_summarize_u64(c: &mut Criterion) {
    let mut group = c.benchmark_group("summarize_u64");

    let sizes: Vec<usize> = vec![10, 100, 1000, 10000];
    for size in sizes {
        let data: Vec<u64> = (0..size as u64).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("elements", size), &data, |b, data| {
            b.iter(|| summarize_u64(black_box(data)));
        });
    }
    group.finish();
}

fn bench_summarize_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("summarize_f64");

    let sizes: Vec<usize> = vec![10, 100, 1000, 10000];
    for size in sizes {
        let data: Vec<f64> = (0..size).map(|i| i as f64).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("elements", size), &data, |b, data| {
            b.iter(|| summarize_f64(black_box(data)));
        });
    }
    group.finish();
}

fn bench_percentile(c: &mut Criterion) {
    let mut group = c.benchmark_group("percentile");

    let sizes: Vec<usize> = vec![10, 100, 1000, 10000];
    for size in sizes {
        let data: Vec<f64> = (0..size).map(|i| i as f64).collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("elements", size), &data, |b, data| {
            b.iter(|| percentile(black_box(data.clone()), 0.5));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_summarize_u64,
    bench_summarize_f64,
    bench_percentile
);
criterion_main!(benches);
