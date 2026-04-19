# Beancount Specification

This directory contains the specification documents for implementing rustledger, a pure Rust implementation of Beancount.

## Documents

### Core Specifications (`core/`)

| File | Description |
|------|-------------|
| [core/syntax.md](core/syntax.md) | Complete language syntax specification |
| [core/grammar.peg](core/grammar.peg) | Formal PEG grammar for parser implementation |
| [core/inventory.md](core/inventory.md) | Inventory, positions, lots, and booking algorithms |
| [core/algorithms.md](core/algorithms.md) | Interpolation, balancing, tolerance algorithms |
| [core/bql.md](core/bql.md) | Beancount Query Language specification |
| [core/options.md](core/options.md) | Configuration options reference |
| [core/validation.md](core/validation.md) | Complete validation error catalog |
| [core/ordering.md](core/ordering.md) | Directive ordering and sort rules |
| [core/wasm-plugins.md](core/wasm-plugins.md) | WASM plugin system design |

### Implementation Specifications (`impl/`)

| File | Description |
|------|-------------|
| [impl/architecture.md](impl/architecture.md) | System architecture and crate structure |
| [impl/decimals.md](impl/decimals.md) | Decimal arithmetic, precision, tolerance rules |
| [impl/api.md](impl/api.md) | Library API design and serialization format |
| [impl/error-recovery.md](impl/error-recovery.md) | Parser error recovery and source locations |
| [impl/properties.md](impl/properties.md) | Property-based testing properties (22 properties) |
| [impl/test-vectors.md](impl/test-vectors.md) | Golden test vectors catalog (220+ cases) |

### Project Specifications (`project/`)

| File | Description |
|------|-------------|
| [project/glossary.md](project/glossary.md) | Definitions of terms |
| [project/performance.md](project/performance.md) | Performance targets and benchmarks |
| [project/compatibility.md](project/compatibility.md) | Python beancount compatibility notes |
| [project/ci.md](project/ci.md) | CI/CD testing strategy |

### Formal Specifications (`tla/`)

| File | Description |
|------|-------------|
| [tla/README.md](tla/README.md) | TLA+ specification overview |
| [tla/RUST_MAPPING.md](tla/RUST_MAPPING.md) | TLA+ to Rust implementation mapping |
| [tla/Conservation.tla](tla/Conservation.tla) | Unit conservation invariants |
| [tla/DoubleEntry.tla](tla/DoubleEntry.tla) | Double-entry bookkeeping invariants |
| [tla/FIFOCorrect.tla](tla/FIFOCorrect.tla) | FIFO lot selection correctness |

### Test Fixtures (`tests/fixtures/`)

| File | Description |
|------|-------------|
| [syntax-edge-cases.beancount](../tests/fixtures/syntax-edge-cases.beancount) | Parser edge cases |
| [booking-scenarios.beancount](../tests/fixtures/booking-scenarios.beancount) | Booking algorithm scenarios |
| [validation-errors.beancount](../tests/fixtures/validation-errors.beancount) | Intentional errors for testing |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         rustledger                             │
├─────────────────────────────────────────────────────────────────┤
│  CLI / WASM / Library API                                        │
├─────────────────────────────────────────────────────────────────┤
│  Parser → Loader → Interpolation → Booking → Validation         │
├─────────────────────────────────────────────────────────────────┤
│  Core Types: Amount, Position, Inventory, Directive              │
├─────────────────────────────────────────────────────────────────┤
│  rust_decimal, chrono, wasmtime                                  │
└─────────────────────────────────────────────────────────────────┘
```

See [impl/architecture.md](impl/architecture.md) for detailed diagrams.

## Implementation Priority

### Phase 1: Core (MVP)

1. **Parser** - Parse full syntax to AST (`core/syntax.md`, `core/grammar.peg`, `impl/error-recovery.md`)
1. **Loader** - Handle includes, collect options (`core/options.md`, `core/ordering.md`)
1. **Core Types** - Amount, Position, Inventory (`core/inventory.md`, `impl/decimals.md`)
1. **Interpolation** - Fill missing posting amounts (`core/algorithms.md`)
1. **Booking** - Lot matching algorithms (`core/inventory.md`, `tla/`)
1. **Validation** - Balance assertions, account lifecycle (`core/validation.md`)

### Phase 2: Tooling

7. **bean-check** - Validate ledger files
1. **BQL Engine** - Query language (`core/bql.md`)
1. **bean-query** - Interactive query REPL

### Phase 3: Extensibility

10. **WASM Plugins** - Plugin runtime (`core/wasm-plugins.md`)
01. **Built-in Plugins** - implicit_prices, etc.

## Key Metrics

| Metric | Python Beancount | rustledger Target |
|--------|------------------|---------------------|
| Parse + validate (10K txns) | 4-6 seconds | < 500ms |
| Memory usage | ~500 MB | < 100 MB |
| Startup time | ~1 second | < 50ms |
| WASM bundle | N/A | < 2 MB gzipped |

See [project/performance.md](project/performance.md) for benchmarks.

## Testing Strategy

| Test Type | Description | Location |
|-----------|-------------|----------|
| Unit Tests | Inline `#[test]` | `crates/*/src/` |
| Integration | Fixture-based | `crates/*/tests/` |
| Golden Tests | 220+ cases from Lima | `tests/fixtures/lima-tests/` |
| Property Tests | 22 proptest properties | `impl/properties.md` |
| Compatibility | Compare vs Python | `tests/compatibility/` |
| Formal | TLA+ model checking | `spec/tla/` |
| Fuzz | libFuzzer targets | `fuzz/` |

Run `scripts/fetch-test-vectors.sh` to download golden test vectors.

See [project/ci.md](project/ci.md) for CI/CD pipeline.

## Quick Reference

- **Terminology**: [project/glossary.md](project/glossary.md)
- **Python differences**: [project/compatibility.md](project/compatibility.md)
- **API usage**: [impl/api.md](impl/api.md)
- **Error codes**: [core/validation.md](core/validation.md)

## Sources

These specs are derived from:

- [Official Beancount Documentation](https://beancount.github.io/docs/)
- [Beancount Source Code](https://github.com/beancount/beancount)
- [beancount-parser-lima](https://github.com/tesujimath/beancount-parser-lima)
- [Beancount v3 Design Doc](https://docs.google.com/document/d/1qPdNXaz5zuDQ8M9uoZFyyFis7hA0G55BEfhWhrVBsfc)
