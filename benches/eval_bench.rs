use criterion::{black_box, criterion_group, criterion_main, Criterion};
use cruncher::{eval, Expr, JIT};
use hashbrown::HashMap;

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

fn jit_eval_benchmark(c: &mut Criterion) {
    let mut jit = JIT::new();
    let compiled = jit.compile("(var1 + var2 * 3) / (2 + 3) - something").unwrap();
    let mut owned: HashMap<String, Vec<f64>> = HashMap::new();
    owned.insert("something".into(), vec![30.0; 1024]);
    owned.insert("var1".into(), vec![10.0; 1024]);
    owned.insert("var2".into(), vec![20.0; 1024]);
    let params: HashMap<String, &[f64]> = owned
        .iter()
        .map(|(k, v)| (k.clone(), v.as_slice()))
        .collect();

    c.bench_function("jit_eval_1024", |b| {
        b.iter(|| compiled(black_box(&params), black_box(1024)).unwrap())
    });
}

criterion_group!(benches, ast_eval_benchmark, jit_eval_benchmark);
criterion_main!(benches);
