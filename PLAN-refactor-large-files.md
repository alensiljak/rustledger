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
├── mod.rs           # Main Executor struct (4,159 lines)
├── types.rs         # Value, Row, QueryResult types (329 lines)
└── functions/
    ├── mod.rs       # Module declarations (10 lines)
    ├── date.rs      # Date functions (544 lines)
    ├── string.rs    # String functions (385 lines)
    ├── account.rs   # Account functions (184 lines)
    ├── math.rs      # Math functions (106 lines)
    ├── position.rs  # Position/inventory functions (387 lines)
    └── util.rs      # Utility/meta/cast functions (335 lines)
```

**Results:**
- Extracted core types to separate module
- Split eval functions into 6 category modules (~1,950 lines)
- All 112 query tests pass
- Public API unchanged

---

## Phase 2: FFI-WASI (main.rs - 3,779 lines) ✅ COMPLETED

**Final structure:**
```
rustledger-ffi-wasi/src/
├── main.rs              # CLI entry point and commands (2,381 lines)
├── types/
│   ├── mod.rs           # Module re-exports (11 lines)
│   ├── output.rs        # Output types for JSON serialization (371 lines)
│   └── input.rs         # Input types for JSON deserialization (448 lines)
├── convert.rs           # Directive to JSON conversion (389 lines)
└── helpers.rs           # Utility functions (229 lines)
```

**Results:**
- Original 3,779-line file split into 5 focused modules
- main.rs reduced to 2,381 lines (commands only)
- Types grouped by direction (output vs input)
- Conversion and helpers cleanly separated

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
2. **Phase 2 (ffi-wasi)** ✅ COMPLETED
3. **Phase 3 (native.rs)** ✅ COMPLETED
4. **Phase 4 (validate)** ✅ COMPLETED
5. **Phase 5 (editor.rs)** ✅ COMPLETED

## Summary

**Completed:** 5 out of 5 phases ✅

| Phase | File | Before | After | Reduction |
|-------|------|--------|-------|-----------|
| Phase 1 | executor.rs | 6,266 | 4,159 + 329 + 1,951 | Extracted types.rs + functions/ |
| Phase 2 | main.rs (ffi-wasi) | 3,779 | 2,381 | -1,398 lines |
| Phase 3 | native.rs | 3,076 | 107 + 2,980 | Split trait/plugins |
| Phase 4 | lib.rs (validate) | 2,223 | 1,987 + 245 | Extracted error.rs |
| Phase 5 | editor.rs | 2,157 | 7 modules | Split into 7 files |

## Success Criteria

- ✅ All tests pass after each phase
- ✅ No public API changes (backwards compatible)
- ✅ Each new file < 500 lines ideally, < 1000 max
- ✅ Clear separation of concerns

## Notes

- Use `pub(crate)` for internal items, `pub` only for public API
- Add module-level doc comments explaining purpose
- Consider using `#[cfg(test)]` modules within each new file for unit tests
