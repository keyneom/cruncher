use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use cruncher::{eval, CompiledExpr, Expr, JIT};
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

fn owned_five(n: usize) -> HashMap<String, Vec<f64>> {
    let mut owned = HashMap::new();
    for (name, base) in [
        ("a", 1.0),
        ("b", 2.0),
        ("c", 3.0),
        ("d", 4.0),
        ("e", 5.0),
    ] {
        owned.insert(name.into(), vec![base; n]);
    }
    owned
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
    let compiled = CompiledExpr::compile("(var1 + var2 * 3) / (2 + 3) - something").unwrap();

    let mut group = c.benchmark_group("batch");
    for n in [1_024usize, 65_536, 1_048_576] {
        let owned = owned_columns(n);
        let params = slice_map(&owned);
        group.throughput(Throughput::Elements(n as u64));

        group.bench_with_input(BenchmarkId::new("ast_eval_batch", n), &n, |b, &n| {
            b.iter(|| expr.eval_batch(black_box(&params), black_box(n)).unwrap())
        });

        group.bench_with_input(BenchmarkId::new("ast_eval_batch_into", n), &n, |b, &n| {
            let mut out = vec![0.0; n];
            b.iter(|| {
                expr.eval_batch_into(black_box(&params), black_box(&mut out))
                    .unwrap()
            })
        });

        group.bench_with_input(BenchmarkId::new("jit_compiled_expr", n), &n, |b, &n| {
            b.iter(|| compiled.eval_batch(black_box(&params), black_box(n)).unwrap())
        });
    }
    group.finish();
}

fn jit_many_params_benchmark(c: &mut Criterion) {
    let compiled = CompiledExpr::compile("a + b + c + d + e").unwrap();
    let mut group = c.benchmark_group("jit_many_params");
    for n in [1_024usize, 65_536] {
        let owned = owned_five(n);
        let params = slice_map(&owned);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("five_params", n), &n, |b, &n| {
            b.iter(|| compiled.eval_batch(black_box(&params), black_box(n)).unwrap())
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

fn folding_parse_benchmark(c: &mut Criterion) {
    // Constant-heavy expression should fold at parse time; measure parse+fold.
    c.bench_function("parse_fold_constant_tree", |b| {
        b.iter(|| Expr::parse(black_box("(1+2*3)^2 / 4 + sqrt(9)")).unwrap())
    });
}

criterion_group!(
    benches,
    ast_eval_benchmark,
    jit_eval_benchmark,
    batch_benchmark,
    jit_many_params_benchmark,
    folding_parse_benchmark
);
criterion_main!(benches);
