# Cruncher Roadmap

Cruncher today is a fast `f64` expression evaluator (AST interpreter + Cranelift JIT)
aimed at bulk numeric evaluation. The long-term goal is to become the core of an
**evaluation engine** for:

- **Curves** — timeseries / tabular numeric columns
- **Statistical distributions** composed in equations
- **Monte Carlo / scenario analysis** (ranges with confidence, not point estimates)

Performance is a product requirement: math on distributions and curves must stay
viable at large sample counts.

## Current state (v0.2.x)

| Capability | Status |
|---|---|
| Parse math expressions (`+ - * / ^`, parens, builtins) | Done |
| AST interpreter (`Expr::eval`) | Done |
| Columnar AST batch (`Expr::eval_batch`) | Done (quick win) |
| Cranelift JIT scalar kernel + host batch loop | Done |
| Rayon parallel JIT batch (large `n`) | Done (quick win) |
| Distribution / curve value types | Not started |
| Sampling / RNG integration | Not started |
| Vectorized / SIMD kernels inside JIT | Not started |
| Equation stacking of distributions | Not started |

### Performance notes

- Prefer **compile once, evaluate many**: JIT amortizes Cranelift cost over large `n`.
- Prefer **columnar inputs** (`HashMap<String, &[f64]>`) over per-row maps.
- JIT still compiles a **scalar** `fn(f64,...) -> f64` and loops in Rust; the next
  big leap is a vectorized loop body (or Cranelift SIMD) so the hot path stays in
  generated code.

## Phase 1 — Solidify the numeric core (near-term)

1. **API surface for bulk eval**
   - Keep `eval` / `Expr` for interactive / scalar use.
   - Promote columnar APIs (`eval_batch`, JIT compile → call) as the default for
     engines and notebooks.
2. **JIT loop improvements**
   - Emit an explicit batch loop in Cranelift (pointers + length) instead of a
     Rust `for` around a scalar fn — cuts call overhead and enables LICM/unroll.
   - Optional: specialize kernels by arity without `Box<dyn Fn>`.
3. **Allocation discipline**
   - Reuse output buffers (`eval_batch_into(&mut [f64])`).
   - Avoid `String` keys on the hot path (intern names / use `&str` contexts).
4. **Benchmarks as gate**
   - Criterion benches for AST scalar, AST batch, JIT batch (1k / 1M), and
     parallel scaling.
   - Track regressions in CI when the project grows a CI pipeline.

## Phase 2 — Curves as first-class values

Design sketch (not implemented):

```text
Curve {
  domain: Axis,          // time, scenario index, ...
  values: Column<f64>,   // dense contiguous buffer
}
```

- Arithmetic on curves is **pointwise** over aligned domains (or broadcast scalars).
- Expression language gains curve-typed variables; evaluator returns `Value::Curve`
  or `Value::Scalar`.
- Storage stays SoA / columnar so Monte Carlo draws and timeseries share the same
  batch machinery from Phase 1.

Milestones:

1. `Value` enum: `Scalar(f64) | Curve(CurveHandle)`
2. Domain alignment rules + clear errors on length mismatch
3. Builtins that reduce curves (`sum`, `mean`, `min`, `max`, later quantiles)

## Phase 3 — Distributions in equations

Design sketch:

```text
Dist::Normal { mean, sd }
Dist::Uniform { lo, hi }
Dist::Empirical { samples: Curve }
```

- Expressions like `profit = price * demand(Normal(100, 15))` parse to an AST whose
  leaves can be distribution nodes.
- Evaluation modes:
  - **Sample**: draw `n` scenarios → columnar `f64` batch (Monte Carlo)
  - **Propagate** (later): analytic or approximate distribution algebra where safe
- Stacking distributions means the compiler/evaluator walks the AST and either
  samples each stochastic leaf with a shared RNG stream or builds a joint sampler.

Disney theme-park example (illustrative):

```text
stay_days ~ Discrete([1,2,3,4], weights)
spend_per_day ~ Normal(mu, sigma)
revenue = stay_days * spend_per_day * guests
```

Output is a **distribution of revenue** (histogram / quantiles), not a single float.

Milestones:

1. `Dist` types + `sample(n, rng) -> Column<f64>`
2. Stochastic leaves in the AST + seeded RNG API
3. Summary helpers: mean, variance, percentile, histogram bins
4. Docs/examples for scenario analysis workflows

## Phase 4 — Vectorized / SIMD evaluation

Once columnar + sample paths are stable:

1. **Cranelift SIMD** or hand-written `std::simd` kernels for pure arithmetic subgraphs
2. **Fused loops** for expression DAGs (common subexpressions across columns)
3. **GPU / optional backends** only if CPU vectorization saturates — keep the
   portable CPU path first

Success metric: multi-million-sample evaluation of moderately complex expressions
(tens of ops, a few stochastic leaves) at interactive or near-interactive latency
on a workstation CPU.

## Phase 5 — Engine integration

- Stable versioned API suitable for embedding in a larger evaluation / planning tool
- Deterministic seeds for reproducibility
- Serialization of expressions + scenario configs
- Optional WASM build for browser notebooks (subset of features if needed)

## Non-goals (for now)

- Full CAS / symbolic algebra
- Arbitrary precision
- Replacing specialized stats packages (use them for fitting; cruncher evaluates)

## Suggested implementation order

```text
batch APIs + benches  →  curve Value  →  Dist + sample
        ↓
   JIT batch loop in IR  →  SIMD hot paths  →  engine polish
```

Quick wins already taken or in progress in this iteration:

- [x] Fix rustc 1.95 diagnostic-width ICE workaround + invalid `#[must_use]` on `Default`
- [x] Replace `lazy_static` with `std::sync::LazyLock`
- [x] `Expr::eval_batch` columnar interpreter path
- [x] Tighter JIT host batch loops + rayon for large `n`
- [x] Criterion benches (extend toward 1M + batch AST)
- [ ] Cranelift-emitted batch loop (next major perf step)
- [ ] `eval_batch_into` / zero-alloc reuse
