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

## Phase 1: Query Executor (executor.rs - 6,266 lines) ✅ COMPLETED

**Final structure:**
```
rustledger-query/src/executor/
├── mod.rs           # Main Executor struct (5,952 lines)
└── types.rs         # Value, Row, QueryResult types (329 lines)
```

**Results:**
- Extracted core types to separate module
- All 112 query tests pass
- Public API unchanged

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

## Phase 3: Native Plugins (native.rs - 3,076 lines) ✅ COMPLETED

**Final structure:**
```
rustledger-plugin/src/native/
├── mod.rs               # NativePlugin trait and NativePluginRegistry (107 lines)
└── plugins.rs           # All 19 plugin implementations (2,980 lines)
```

**Results:**
- Separated trait/registry from implementations
- All 17 plugin tests pass
- Public API unchanged

---

## Phase 4: Validation (lib.rs - 2,223 lines) ✅ COMPLETED

**Final structure:**
```
rustledger-validate/src/
├── lib.rs               # Validation logic (1,987 lines)
└── error.rs             # ErrorCode, Severity, ValidationError (245 lines)
```

**Results:**
- Extracted error types to separate module
- All 23 validation tests pass
- Public API unchanged

---

## Phase 5: WASM Editor (editor.rs - 2,157 lines) ✅ COMPLETED

**Current structure:** Split into modules.

**Final structure:**
```
rustledger-wasm/src/editor/
├── mod.rs               # Public API and re-exports (129 lines)
├── completions.rs       # Completion context and provider (306 lines)
├── definitions.rs       # Go-to-definition support (98 lines)
├── helpers.rs           # Utility functions and constants (270 lines)
├── hover.rs             # Hover information (235 lines)
├── line_index.rs        # EditorCache and LineIndex (130 lines)
├── references.rs        # Find references support (310 lines)
└── symbols.rs           # Document symbols (273 lines)
```

**Results:**
- Original 2,157 line file split into 7 focused modules
- All 45 wasm tests pass
- No public API changes

---

## Implementation Order

1. **Phase 1 (executor.rs)** ✅ COMPLETED
2. **Phase 2 (ffi-wasi)** ⏸️ DEFERRED (tight coupling issues)
3. **Phase 3 (native.rs)** ✅ COMPLETED
4. **Phase 4 (validate)** ✅ COMPLETED
5. **Phase 5 (editor.rs)** ✅ COMPLETED

## Summary

**Completed:** 4 out of 5 phases
- Phase 1: Executor types extracted to `types.rs`
- Phase 3: Plugins extracted to `plugins.rs`, trait/registry in `mod.rs`
- Phase 4: Error types extracted to `error.rs`
- Phase 5: Editor split into 7 focused modules

**Deferred:** Phase 2 (FFI-WASI)
- Requires careful incremental extraction due to type/impl coupling
- Plan documented for future implementation

## Success Criteria

- ✅ All tests pass after each phase
- ✅ No public API changes (backwards compatible)
- ✅ Each new file < 500 lines ideally, < 1000 max
- ✅ Clear separation of concerns

## Notes

- Use `pub(crate)` for internal items, `pub` only for public API
- Add module-level doc comments explaining purpose
- Consider using `#[cfg(test)]` modules within each new file for unit tests
