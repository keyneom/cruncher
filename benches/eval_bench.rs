use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use cruncher::{eval, Expr, JIT};
use hashbrown::HashMap;

fn owned_columns(n: usize) -> HashMap<String, Vec<f64>> {
    let mut owned = HashMap::new();
    owned.insert("something".into(), vec![30.0; n]);
    owned.insert("var1".into(), vec![10.0; n]);
    owned.insert("var2".into(), vec![20.0; n]);
    owned
}

fn slice_map(owned: &HashMap<String, Vec<f64>>) -> HashMap<String, &[f64]> {
    owned
        .iter()
        .map(|(k, v)| (k.clone(), v.as_slice()))
        .collect()
}

fn ast_eval_benchmark(c: &mut Criterion) {
    let expr = Expr::parse("(var1 + var2 * 3) / (2 + 3) - something").unwrap();
    let mut ctx = HashMap::new();
    ctx.insert("var1".into(), 10.0);
    ctx.insert("var2".into(), 20.0);
    ctx.insert("something".into(), 30.0);

    c.bench_function("ast_eval_simple", |b| {
        b.iter(|| expr.eval(black_box(&ctx)).unwrap())
    });

    c.bench_function("eval_parse_each_time", |b| {
        b.iter(|| eval(black_box("3 + 5 * 2"), black_box(&HashMap::new())).unwrap())
    });
}

fn batch_benchmark(c: &mut Criterion) {
    let expr = Expr::parse("(var1 + var2 * 3) / (2 + 3) - something").unwrap();
    let mut jit = JIT::new();
    let compiled = jit
        .compile("(var1 + var2 * 3) / (2 + 3) - something")
        .unwrap();

    let mut group = c.benchmark_group("batch");
    for n in [1_024usize, 65_536, 1_048_576] {
        let owned = owned_columns(n);
        let params = slice_map(&owned);
        group.throughput(Throughput::Elements(n as u64));

        group.bench_with_input(BenchmarkId::new("ast_eval_batch", n), &n, |b, &n| {
            b.iter(|| expr.eval_batch(black_box(&params), black_box(n)).unwrap())
        });

        group.bench_with_input(BenchmarkId::new("jit_eval", n), &n, |b, &n| {
            b.iter(|| compiled(black_box(&params), black_box(n)).unwrap())
        });
    }
    group.finish();
}

fn jit_eval_benchmark(c: &mut Criterion) {
    let mut jit = JIT::new();
    let compiled = jit
        .compile("(var1 + var2 * 3) / (2 + 3) - something")
        .unwrap();
    let owned = owned_columns(1024);
    let params = slice_map(&owned);

    c.bench_function("jit_eval_1024", |b| {
        b.iter(|| compiled(black_box(&params), black_box(1024)).unwrap())
    });
}

criterion_group!(benches, ast_eval_benchmark, jit_eval_benchmark, batch_benchmark);
criterion_main!(benches);
