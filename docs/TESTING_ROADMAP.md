# Testing Infrastructure Roadmap

This document outlines a phased plan to elevate rustledger's testing infrastructure to best-in-class status.

## Current State

rustledger already has excellent testing foundations:

| Category | Status | Details |
|----------|--------|---------|
| Unit/Integration Tests | ✅ | 99 files with `#[test]`, 14 integration test files |
| Property Testing | ✅ | proptest with TLA+ invariant verification |
| Fuzzing | ✅ | 2 targets (raw + structured), all 16 directive types |
| TLA+ Model Checking | ✅ | 19 specifications covering core invariants |
| Compatibility Testing | ✅ | 800+ files, 3 dimensions (check/BQL/AST) |
| Performance Testing | ✅ | Cross-tool benchmarks + criterion micro-benchmarks |
| Security Scanning | ✅ | gitleaks, cargo-deny, cargo-vet, CodeQL, dependency-review |
| Snapshot Testing | ✅ | insta for parser output |

**Current grade: A-** (top 5% of Rust projects)

## Gaps Identified

### From Internal Review
1. Fuzzing not in CI (manual only)
2. Error message quality not tested in compat suite
3. BQL test coverage shallow (11 queries, 50 files)
4. Criterion benchmarks not in CI
5. WASM crate untested
6. No coverage reporting

### From Industry Comparison
1. No Miri for undefined behavior detection
2. No OSS-Fuzz for continuous fuzzing
3. No cargo-nextest for faster test execution
4. No mutation testing
5. No Kani formal verification (despite having TLA+ specs)

---

## Phase 1: Quick Wins (1-2 days)

Low-effort, high-impact improvements that can be done immediately.

### 1.1 Add Miri to CI

**Why**: Detects undefined behavior in unsafe code that sanitizers miss.

**File**: `.github/workflows/ci.yml`

```yaml
miri:
  name: Miri
  runs-on: ubuntu-latest
  # Run weekly - Miri is slow and nightly-only
  if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@nightly
      with:
        components: miri
    - name: Run Miri
      run: cargo +nightly miri test --all-features
      env:
        MIRIFLAGS: -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance
```

**Effort**: 1 hour

### 1.2 Switch to cargo-nextest

**Why**: 2-3x faster test execution, better failure isolation, cleaner output.

**File**: `.github/workflows/ci.yml`

```yaml
- name: Install nextest
  uses: taiki-e/install-action@nextest

- name: Test
  run: cargo nextest run --all-features
```

**Effort**: 30 minutes

### 1.3 Add Coverage Reporting

**Why**: Visibility into test coverage trends.

**File**: `.github/workflows/ci.yml` (already has Code Coverage job, just add upload)

```yaml
- name: Upload to Codecov
  uses: codecov/codecov-action@v4
  with:
    files: lcov.info
    fail_ci_if_error: false
```

**Effort**: 30 minutes

### 1.4 Run Criterion Benchmarks in CI

**Why**: Detect performance regressions in micro-benchmarks.

**File**: `.github/workflows/bench-pr.yml`

```yaml
- name: Run Criterion benchmarks
  run: cargo bench --all-features -- --noplot
```

**Effort**: 1 hour

---

## Phase 2: Fuzzing Infrastructure (3-5 days)

Elevate fuzzing from manual to continuous.

### 2.1 Fuzzing in CI (PR-level)

**Why**: Catch regressions before merge.

**File**: `.github/workflows/fuzz.yml`

```yaml
name: Fuzz

on:
  pull_request:
    paths:
      - 'crates/rustledger-parser/**'
  schedule:
    - cron: '0 4 * * *'  # Nightly at 4 AM UTC

jobs:
  fuzz:
    name: Fuzz ${{ matrix.target }}
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target: [fuzz_parse, fuzz_parse_line]
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@nightly
      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz
      - name: Run fuzzer
        run: |
          cd crates/rustledger-parser
          cargo +nightly fuzz run ${{ matrix.target }} -- \
            -max_total_time=600 \
            -max_len=4096
      - name: Upload corpus
        uses: actions/upload-artifact@v4
        with:
          name: corpus-${{ matrix.target }}
          path: crates/rustledger-parser/fuzz/corpus/${{ matrix.target }}
```

**Effort**: 2 hours

### 2.2 OSS-Fuzz Integration

**Why**: Google runs your fuzzers 24/7 for free, files bugs automatically.

**Steps**:
1. Create `projects/rustledger/` in google/oss-fuzz fork
2. Add `Dockerfile`, `build.sh`, `project.yaml`
3. Submit PR to google/oss-fuzz

**File**: `oss-fuzz/Dockerfile` (to create in fork)

```dockerfile
FROM gcr.io/oss-fuzz-base/base-builder-rust
RUN git clone --depth 1 https://github.com/rustledger/rustledger
WORKDIR rustledger
COPY build.sh $SRC/
```

**File**: `oss-fuzz/build.sh`

```bash
#!/bin/bash -eu
cd $SRC/rustledger/crates/rustledger-parser
cargo +nightly fuzz build
cp fuzz/target/x86_64-unknown-linux-gnu/release/fuzz_parse $OUT/
cp fuzz/target/x86_64-unknown-linux-gnu/release/fuzz_parse_line $OUT/
```

**Effort**: 1 day

### 2.3 Add Fuzzing for Query Engine

**Why**: BQL parser/executor could have bugs not covered by current targets.

**File**: `crates/rustledger-query/fuzz/fuzz_targets/fuzz_query.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use rustledger_query::parse_query;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        let _ = parse_query(input);
    }
});
```

**Effort**: 2 hours

---

## Phase 3: Compatibility Testing Enhancements (2-3 days)

Address gaps in the compatibility testing suite.

### 3.1 Error Message Quality Testing

**Why**: Currently only counts errors, doesn't verify messages are helpful.

**File**: `scripts/compat-error-quality.py`

```python
#!/usr/bin/env python3
"""Compare error message quality between bean-check and rledger check."""

import subprocess
import json
from pathlib import Path

# Known-bad inputs with expected error patterns
ERROR_CASES = [
    ("invalid_date.beancount", "Invalid date"),
    ("unclosed_string.beancount", "Unterminated string"),
    ("bad_account.beancount", "Invalid account"),
    # ... more cases
]

def compare_errors(file: Path):
    bean_result = subprocess.run(
        ["bean-check", str(file)],
        capture_output=True, text=True
    )
    rust_result = subprocess.run(
        ["rledger", "check", str(file)],
        capture_output=True, text=True
    )

    # Compare error locations, types, and helpfulness
    return {
        "file": str(file),
        "python_stderr": bean_result.stderr,
        "rust_stderr": rust_result.stderr,
        "location_match": compare_locations(bean_result.stderr, rust_result.stderr),
        "type_match": compare_error_types(bean_result.stderr, rust_result.stderr),
    }
```

**Effort**: 4 hours

### 3.2 Expand BQL Test Coverage

**Why**: Only 11 queries tested; query engine has 100+ functions.

**File**: `scripts/compat-bql-comprehensive.sh`

Add tests for:
- All aggregate functions (SUM, COUNT, FIRST, LAST, MIN, MAX, etc.)
- Date functions (YEAR, MONTH, DAY, etc.)
- String functions
- Type conversion functions
- Edge cases (NULL, empty results, large results)

**Effort**: 1 day

### 3.3 Remove BQL 50-File Limit

**Why**: "Due to test execution time" comment suggests scaling problem.

**Solution**:
- Run BQL tests in parallel
- Use sampling for nightly (all files) vs PR (subset)
- Cache query results

**Effort**: 4 hours

---

## Phase 4: Formal Verification Bridge (3-5 days)

Connect TLA+ specs to Rust implementation.

### 4.1 Kani Proof Harnesses

**Why**: Verify Rust code satisfies TLA+ invariants directly.

**File**: `crates/rustledger-core/src/inventory_proofs.rs`

```rust
#[cfg(kani)]
mod proofs {
    use super::*;

    /// Proof: Conservation invariant holds for all add operations
    #[kani::proof]
    #[kani::unwind(10)]
    fn proof_conservation_add() {
        let mut inventory = Inventory::new();
        let amount: i64 = kani::any();
        kani::assume(amount > 0 && amount < 1_000_000);

        let position = Position::new(Amount::new(amount.into(), "USD"));
        let before_total = inventory.total("USD");
        inventory.add(position.clone());
        let after_total = inventory.total("USD");

        // Conservation: total changes by exactly the added amount
        kani::assert(after_total - before_total == position.units());
    }

    /// Proof: FIFO always selects oldest lot
    #[kani::proof]
    fn proof_fifo_oldest() {
        // ... implement based on FIFOCorrect.tla
    }
}
```

**File**: `.github/workflows/kani.yml`

```yaml
name: Kani Verification

on:
  push:
    paths:
      - 'crates/rustledger-core/src/**'
      - 'crates/rustledger-booking/src/**'
  schedule:
    - cron: '0 5 * * 0'  # Weekly on Sunday

jobs:
  kani:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: model-checking/kani-github-action@v1
        with:
          args: --all-features
```

**Effort**: 3-5 days (significant but high value)

### 4.2 TLA+ Trace to Test Automation

**Why**: `trace_to_rust_test.py` exists but isn't automated.

**File**: `.github/workflows/tla.yml` (extend existing)

```yaml
- name: Generate regression tests from traces
  if: failure()
  run: |
    python scripts/trace_to_rust_test.py \
      --trace tlc-output/trace.json \
      --output crates/rustledger-core/tests/generated_regression.rs
```

**Effort**: 4 hours

---

## Phase 5: Mutation Testing (1-2 days)

Find undertested code paths.

### 5.1 Monthly Mutation Testing

**Why**: Coverage metrics lie. Mutation testing shows if tests actually verify behavior.

**File**: `.github/workflows/mutation.yml`

```yaml
name: Mutation Testing

on:
  schedule:
    - cron: '0 6 1 * *'  # Monthly on 1st at 6 AM
  workflow_dispatch:

jobs:
  mutants:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-mutants
        run: cargo install cargo-mutants

      - name: Run mutation testing
        run: |
          cargo mutants --timeout 300 --jobs 4 \
            --package rustledger-core \
            --package rustledger-parser \
            --package rustledger-booking \
            -- --all-features

      - name: Upload report
        uses: actions/upload-artifact@v4
        with:
          name: mutation-report
          path: mutants.out/

      - name: Check mutation score
        run: |
          SCORE=$(cargo mutants --timeout 300 --jobs 4 --package rustledger-core -- --all-features 2>&1 | grep -oP '\d+(?=% killed)')
          if [ "$SCORE" -lt 70 ]; then
            echo "::warning::Mutation score below 70%: $SCORE%"
          fi
```

**Effort**: 2 hours

---

## Phase 6: WASM Testing (1 day)

Test the `rustledger-wasm` crate.

### 6.1 Node.js Tests

**File**: `crates/rustledger-wasm/tests/node.rs`

```rust
#[wasm_bindgen_test]
fn test_parse_simple() {
    let result = parse("2024-01-01 open Assets:Bank");
    assert!(result.is_ok());
}
```

**File**: `.github/workflows/ci.yml` (add job)

```yaml
wasm:
  name: WASM Tests
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: dtolnay/rust-toolchain@stable
      with:
        targets: wasm32-unknown-unknown
    - name: Install wasm-pack
      run: cargo install wasm-pack
    - name: Run WASM tests
      run: |
        cd crates/rustledger-wasm
        wasm-pack test --node
```

**Effort**: 4 hours

---

## Implementation Timeline

| Phase | Effort | Priority | Cumulative Impact |
|-------|--------|----------|-------------------|
| Phase 1: Quick Wins | 1-2 days | Critical | A- → A |
| Phase 2: Fuzzing | 3-5 days | High | Continuous bug finding |
| Phase 3: Compat Enhancements | 2-3 days | Medium | Better beancount parity |
| Phase 4: Formal Verification | 3-5 days | Medium | Provable correctness |
| Phase 5: Mutation Testing | 1-2 days | Low | Find undertested code |
| Phase 6: WASM Testing | 1 day | Low | Complete coverage |

**Total**: ~2-3 weeks of focused work

---

## Success Metrics

After completing all phases:

| Metric | Current | Target |
|--------|---------|--------|
| Test types | 8 | 12 |
| Fuzzing frequency | Manual | Continuous (OSS-Fuzz) |
| UB detection | None | Miri weekly |
| Mutation score | Unknown | >70% |
| CI time | ~15 min | ~10 min (nextest) |
| BQL test coverage | 11 queries | 50+ queries |
| WASM tests | 0 | Full coverage |
| Formal verification | TLA+ only | TLA+ + Kani |

**Target grade: A+** (top 1% of Rust projects)

---

## Appendix: File Changes Summary

### New Files
- `.github/workflows/fuzz.yml`
- `.github/workflows/kani.yml`
- `.github/workflows/mutation.yml`
- `crates/rustledger-query/fuzz/` (new fuzz target)
- `crates/rustledger-core/src/inventory_proofs.rs`
- `scripts/compat-error-quality.py`

### Modified Files
- `.github/workflows/ci.yml` (Miri, nextest, coverage, WASM)
- `.github/workflows/bench-pr.yml` (Criterion)
- `.github/workflows/tla.yml` (trace automation)
- `scripts/compat-bql-test.sh` (expanded queries)

### External PRs
- google/oss-fuzz (new project integration)
