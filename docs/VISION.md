# Cruncher — product vision

## What we're building

A **high-performance expression evaluation engine** in Rust. Today: parse math expressions, interpret or JIT-compile them, and evaluate over **columnar batches** (millions of rows). Tomorrow: the core of **scenario and Monte Carlo analysis** where business variables are **distributions**, not single point estimates.

## User stories

1. **As an analyst**, I want to write formulas like `revenue = stay_days * spend_per_day * guests` and evaluate them over **many scenarios at once**, not one spreadsheet cell at a time.
2. **As a planner**, I want variables to be **distributions** (e.g. length of stay between 1–4 days with uncertainty) instead of “we assume 3 days” — so I see a **distribution of outcomes**, not one misleading number.
3. **As a simulation author**, I want to stack **curves** (timeseries), **distributions**, and scalar parameters in one expression language and get histograms / percentiles / means.
4. **As a platform builder**, I need evaluation so fast that Monte Carlo at scale is **viable and meaningful** — compile once, run JIT or vectorized batches over huge `n`.
5. **As a maintainer**, I want modern Rust, passing tests, and benchmarks that gate regressions.

## Illustrative scenario (Disney theme park)

Profitability depends on uncertain inputs. Example (see [ROADMAP.md](ROADMAP.md) for full workflow):

- `stay_days ~ Uniform(1, 4)` — not a single “3 days”
- `spend_per_day ~ Normal(μ, σ)`
- `revenue = stay_days * spend_per_day * party_size`

Output: distribution of revenue (quantiles, histogram), not one float.

## Technical direction

Detailed phases, API status, and milestones: **[ROADMAP.md](ROADMAP.md)**.

Performance principles:

- Compile once (`CompiledExpr` / JIT), evaluate many.
- Columnar `HashMap<String, &[f64]>` inputs; avoid per-row allocations.
- Parallel batch eval where it pays off; eventual SIMD / vectorized JIT loops.
