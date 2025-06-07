use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_packet_parsing(c: &mut Criterion) {
    // TODO: Add benchmark when zero-copy parsing is implemented
    c.bench_function("parse dns packet", |b| {
        b.iter(|| {
            // Placeholder
            black_box(42);
        });
    });
}

criterion_group!(benches, bench_packet_parsing);
criterion_main!(benches);
