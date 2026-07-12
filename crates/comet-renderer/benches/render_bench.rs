use comet_core::Terminal;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_render_80x24(c: &mut Criterion) {
    c.bench_function("render_80x24", |b| {
        b.iter(|| {
            let mut term = Terminal::new(80, 24);
            term.write(&"The quick brown fox jumps over the lazy dog. ".repeat(10));
            black_box(term);
        })
    });
}

fn bench_render_200x60(c: &mut Criterion) {
    c.bench_function("render_200x60", |b| {
        b.iter(|| {
            let mut term = Terminal::new(200, 60);
            term.write(&"Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(50));
            black_box(term);
        })
    });
}

fn bench_terminal_write(c: &mut Criterion) {
    c.bench_function("terminal_write_1000_chars", |b| {
        b.iter(|| {
            let mut term = Terminal::new(80, 24);
            term.write(&"The quick brown fox jumps over the lazy dog. ".repeat(20));
            black_box(term);
        })
    });
}

fn bench_damage_tracker(c: &mut Criterion) {
    c.bench_function("damage_tracker_add_1000", |b| {
        b.iter(|| {
            let tracker = comet_renderer::DamageTracker::new(80, 24);
            for i in 0..1000 {
                tracker.add_cell(i % 80, i % 24);
            }
            black_box(tracker);
        })
    });
}

criterion_group!(
    benches,
    bench_render_80x24,
    bench_render_200x60,
    bench_terminal_write,
    bench_damage_tracker
);

criterion_main!(benches);
