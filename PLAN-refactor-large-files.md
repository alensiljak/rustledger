# Plan: Split Large Files into Modules

## Overview

This refactoring plan addresses the largest source files in the rustledger codebase to improve maintainability, readability, and compile times. Files are prioritized by size and complexity.

## Files to Refactor

| File | Lines | Priority |
|------|-------|----------|
| `rustledger-query/src/executor.rs` | 6,266 | High |
| `rustledger-ffi-wasi/src/main.rs` | 3,779 | High |
| `rustledger-plugin/src/native.rs` | 3,076 | Medium |
| `rustledger-validate/src/lib.rs` | 2,223 | Medium |
| `rustledger-wasm/src/editor.rs` | 2,157 | Medium |

---

## Phase 1: Query Executor (executor.rs - 6,266 lines)

**Current structure:** Single file with all BQL execution logic.

**Proposed structure:**
```
rustledger-query/src/
├── executor/
│   ├── mod.rs           # Main Executor struct and entry points
│   ├── expressions.rs   # Expression evaluation (BinaryOp, UnaryOp, etc.)
│   ├── functions.rs     # Scalar functions (SUBSTR, UPPER, DATE_ADD, etc.)
│   ├── aggregates.rs    # Aggregate functions (SUM, COUNT, AVG, etc.)
│   ├── window.rs        # Window functions (ROW_NUMBER, RANK, LAG, etc.)
│   ├── context.rs       # ExecutionContext and variable scoping
│   └── types.rs         # Value type, Row, Column definitions
└── executor.rs          # Re-export from executor/mod.rs (backwards compat)
```

**Steps:**
1. Create `executor/` directory
2. Extract `Value` enum and related types to `types.rs`
3. Extract expression evaluation to `expressions.rs`
4. Extract scalar functions to `functions.rs`
5. Extract aggregate functions to `aggregates.rs`
6. Extract window functions to `window.rs`
7. Keep main `Executor` struct in `mod.rs`
8. Add re-exports for backwards compatibility

---

## Phase 2: FFI-WASI (main.rs - 3,779 lines)

**Status:** Deferred - requires careful extraction due to tight coupling between types, conversion functions, and commands. The impl blocks (Meta::new, TypedValue::from_meta_value) depend on conversion functions, and commands depend on both.

**Recommended approach:** Extract incrementally:
1. First extract just `types.rs` with struct definitions (no impl blocks)
2. Keep conversion functions in main.rs initially
3. Then extract conversion functions to `convert.rs` once types are stable
4. Finally extract commands one at a time

**Current structure:** Single main.rs with all WASI FFI logic.

**Proposed structure:**
```
rustledger-ffi-wasi/src/
├── main.rs              # CLI entry point and command dispatch
├── commands/
│   ├── mod.rs           # Command enum and dispatcher
│   ├── check.rs         # check, check-json commands
│   ├── query.rs         # query, query-json commands
│   ├── format.rs        # format command
│   ├── parse.rs         # parse, clamp-entries commands
│   └── report.rs        # report commands
├── convert.rs           # Directive to JSON conversion
├── types.rs             # FFI-specific types and serde helpers
└── error.rs             # Error types and handling
```

**Steps:**
1. Create `commands/` directory
2. Extract command handling into separate files
3. Extract JSON conversion logic to `convert.rs`
4. Extract type definitions to `types.rs`
5. Slim down `main.rs` to just CLI parsing and dispatch

---

## Phase 3: Native Plugins (native.rs - 3,076 lines)

**Current structure:** Single file with all 20 native plugins.

**Proposed structure:**
```
rustledger-plugin/src/native/
├── mod.rs               # NativePluginRegistry and trait
├── auto_accounts.rs     # beancount.plugins.auto_accounts
├── check_average_cost.rs
├── check_commodity.rs
├── check_drained.rs
├── coherent_cost.rs
├── commodity_attr.rs
├── currency_accounts.rs
├── implicit_prices.rs
├── leafonly.rs
├── noduplicates.rs
├── nounused.rs
├── onecommodity.rs
├── pedantic.rs
├── sellgains.rs
├── unique_prices.rs
├── unrealized.rs
└── ... (remaining plugins)
```

**Steps:**
1. Create `native/` directory
2. Move `NativePlugin` trait and registry to `mod.rs`
3. Extract each plugin to its own file
4. Update imports and re-exports

---

## Phase 4: Validation (lib.rs - 2,223 lines)

**Current structure:** Single lib.rs with all validation logic.

**Proposed structure:**
```
rustledger-validate/src/
├── lib.rs               # Public API and ValidationError enum
├── validators/
│   ├── mod.rs           # Validator trait and runner
│   ├── accounts.rs      # Account-related validations
│   ├── balances.rs      # Balance check validations
│   ├── commodities.rs   # Commodity validations
│   ├── documents.rs     # Document validations
│   └── transactions.rs  # Transaction validations
└── error.rs             # ValidationError and related types
```

**Steps:**
1. Create `validators/` directory
2. Group validators by category
3. Keep public API stable in `lib.rs`

---

## Phase 5: WASM Editor (editor.rs - 2,157 lines)

**Current structure:** Single file with all editor support logic.

**Proposed structure:**
```
rustledger-wasm/src/editor/
├── mod.rs               # Public API
├── completions.rs       # Completion provider
├── hover.rs             # Hover information
├── symbols.rs           # Document symbols
├── references.rs        # Find references
├── diagnostics.rs       # Error diagnostics
└── line_index.rs        # Line/column utilities
```

**Steps:**
1. Create `editor/` directory
2. Extract each feature to its own file
3. Maintain backwards compat via re-exports

---

## Implementation Order

1. **Phase 1 (executor.rs)** - Highest impact, most complex
2. **Phase 2 (ffi-wasi)** - New code, good to establish patterns early
3. **Phase 3 (native.rs)** - Straightforward, each plugin is independent
4. **Phase 4 (validate)** - Medium complexity
5. **Phase 5 (editor.rs)** - Lower priority, WASM-specific

## Success Criteria

- All tests pass after each phase
- No public API changes (backwards compatible)
- Each new file < 500 lines ideally, < 1000 max
- Improved compile times for incremental builds
- Clear separation of concerns

## Notes

- Use `pub(crate)` for internal items, `pub` only for public API
- Add module-level doc comments explaining purpose
- Consider using `#[cfg(test)]` modules within each new file for unit tests
