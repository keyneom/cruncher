# Cruncher Roadmap

Cruncher today is a fast `f64` expression evaluator (AST interpreter + Cranelift JIT)
aimed at bulk numeric evaluation. The long-term goal is to become the core of an
**evaluation engine** for:

- **Curves** — timeseries / tabular numeric columns
- **Statistical distributions** composed in equations
- **Monte Carlo / scenario analysis** (ranges with confidence, not point estimates)

Performance is a product requirement: math on distributions and curves must stay
viable at large sample counts.

## Current state (v0.3.x)

| Capability | Status |
|---|---|
| Parse math expressions (`+ - * / ^`, parens, builtins) | Done (Pratt parser) |
| Structured parse/name errors with byte spans | Done |
| AST interpreter (`Expr::eval`) | Done |
| Columnar AST batch (`Expr::eval_batch` / `eval_batch_into`) | Done |
| Cranelift JIT scalar kernel + host batch loop | Done |
| JIT packed ABI for >4 parameters | Done |
| `CompiledExpr` batch API | Done |
| Rayon parallel JIT batch (large `n`) | Done |
| Constant folding + algebraic identities | Done (quick wins) |
| `Curve` / `Dist` / `Distribution` MVP stubs | Done (sampling only) |
| Vectorized / SIMD kernels inside JIT | Not started |
| Equation stacking of distributions in the language | Not started |

### Performance notes

- Prefer **compile once, evaluate many**: JIT amortizes Cranelift cost over large `n`.
- Prefer **columnar inputs** (`HashMap<String, &[f64]>`) over per-row maps.
- JIT still compiles a **scalar** `fn(...) -> f64` (or packed `*const f64`) and loops
  in Rust; the next big leap is a vectorized loop body (or Cranelift SIMD) so the
  hot path stays in generated code.

## Phase 1 — Solidify the numeric core (near-term)

1. **API surface for bulk eval**
   - Keep `eval` / `Expr` for interactive / scalar use.
   - Promote columnar APIs (`eval_batch`, `CompiledExpr`) as the default for
     engines and notebooks.
2. **JIT loop improvements**
   - Emit an explicit batch loop in Cranelift (pointers + length) instead of a
     Rust `for` around a scalar fn — cuts call overhead and enables LICM/unroll.
   - Optional: specialize kernels by arity without `Box<dyn Fn>`.
3. **Allocation discipline**
   - Reuse output buffers (`eval_batch_into(&mut [f64])`) — available on AST path;
     JIT `CompiledExpr::eval_batch_into` still allocates internally today.
   - Avoid `String` keys on the hot path (intern names / use `&str` contexts).
4. **Benchmarks as gate**
   - Criterion benches for AST scalar, AST batch, JIT batch (1k / 1M), and
     parallel scaling.
   - CI runs `cargo test` and a `--quick` criterion pass.

## Phase 2 — Curves as first-class values

Design sketch:

```text
Curve {
  domain: Axis,          // time, scenario index, ...
  values: Column<f64>,   // dense contiguous buffer
}
```

MVP in-tree today (`src/value.rs`):

```rust
pub struct Curve { pub values: Vec<f64> }
pub enum Value { Scalar(f64), Curve(Curve) }
```

- Arithmetic on curves is **pointwise** over aligned domains (or broadcast scalars).
- Expression language gains curve-typed variables; evaluator returns `Value::Curve`
  or `Value::Scalar`.
- Storage stays SoA / columnar so Monte Carlo draws and timeseries share the same
  batch machinery from Phase 1.

Milestones:

1. `Value` enum: `Scalar(f64) | Curve(Curve)` — stub present
2. Domain alignment rules + clear errors on length mismatch
3. Builtins that reduce curves (`sum`, `mean`, `min`, `max`, later quantiles)
4. Wire `Value` through `Expr` (today expressions remain `f64`-only)

## Phase 3 — Distributions in equations

Design sketch:

```text
Dist::Normal { mean, sd }
Dist::Uniform { lo, hi }
Dist::Empirical { samples: Curve }
```

MVP in-tree today:

- `Distribution` trait with `sample_into`
- `Dist` enum + `Lcg64` stub RNG (no `rand` dependency yet)
- Helpers that produce columnar samples for batch eval

- Expressions like `profit = price * demand(Normal(100, 15))` parse to an AST whose
  leaves can be distribution nodes.
- Evaluation modes:
  - **Sample**: draw `n` scenarios → columnar `f64` batch (Monte Carlo)
  - **Propagate** (later): analytic or approximate distribution algebra where safe
- Stacking distributions means the compiler/evaluator walks the AST and either
  samples each stochastic leaf with a shared RNG stream or builds a joint sampler.

### Disney theme-park scenario (illustrative)

Guest revenue for a stay is a distribution, not a point estimate:

```text
stay_days     ~ Uniform(1, 4)          # continuous stand-in for discrete nights
spend_per_day ~ Normal(mu, sigma)
guests        = 4                      # party size (scalar)
revenue       = stay_days * spend_per_day * guests
```

Workflow with the MVP types:

1. Sample `stay_days` and `spend_per_day` columns (`Dist::sample` / `theme_park_revenue_columns`).
2. Bind those columns (plus any scalars broadcast as length-`n` columns) into
   `Expr::eval_batch` or `CompiledExpr::eval_batch` for the algebraic part.
3. Summarize the revenue curve (mean, percentiles, histogram) — summary helpers
   beyond `Curve::mean` are still to-do.

Output is a **distribution of revenue** (histogram / quantiles), not a single float.

Milestones:

1. `Dist` types + `sample(n, rng) -> Curve` — stub present
2. Stochastic leaves in the AST + seeded RNG API
3. Summary helpers: mean, variance, percentile, histogram bins
4. Docs/examples for scenario analysis workflows — this section + `theme_park_revenue_columns`

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

Checklist:

- [x] Fix rustc diagnostic-width ICE workaround (`.cargo/config.toml`)
- [x] Replace `lazy_static` with `std::sync::LazyLock`
- [x] `Expr::eval_batch` / `eval_batch_into` columnar interpreter path
- [x] Tighter JIT host batch loops + rayon for large `n`
- [x] Criterion benches (1k / 64k / 1M + compile-once)
- [x] Pratt parser path
- [x] Structured errors with spans
- [x] JIT packed ABI for >4 parameters
- [x] `CompiledExpr` batch API
- [x] Constant folding quick wins (`*0`, `*1`, `+0`, `^0`, `^1`, …)
- [x] `Curve` / `Dist` / `Distribution` MVP + theme-park helper
- [x] CI workflow
- [ ] Cranelift-emitted batch loop (next major perf step)
- [ ] Zero-alloc JIT `eval_batch_into`
- [ ] Stochastic AST leaves + language syntax for distributions
