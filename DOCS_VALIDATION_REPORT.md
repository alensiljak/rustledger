# Documentation Validation Report

## Summary of Fixes

### Phase 1: Known Issues - COMPLETED
| File | Issue | Status |
|------|-------|--------|
| `crates/rustledger-plugin/src/lib.rs:17` | Said "14 Built-in Plugins", actual is 30 | FIXED |
| `crates/rustledger-validate/README.md` | Claims 27 error codes | VERIFIED CORRECT |

### Phase 2: CLI Command Documentation - COMPLETED
All 9 CLI commands validated against `--help` output.

| Document | Status |
|----------|--------|
| format.md | FIXED - added 8 missing options |
| extract.md | FIXED - corrected option names, added CLI options |
| price.md | FIXED - removed wrong options, added correct ones |
| doctor.md | FIXED - added generate-synthetic subcommand |

### Phase 3: BQL Reference - COMPLETED
Validated all BQL features by running queries.

**Fixed:**
- Removed `~*` operator (doesn't exist; `~` is already case-insensitive)
- Removed `today() - 30` syntax (date arithmetic not supported)
- Fixed FILTER clause example (not implemented)

**Code bugs found (not doc issues):**
- `quarter()` and `weekday()` don't work with `FROM #postings` due to missing implementations in `evaluate_function_on_values`

### Phase 4: Syntax Reference - COMPLETED
All directive examples validated with `rledger check`.

### Phase 5: Plugin Validation - SKIPPED
Already fixed in PR #620.

### Phase 6: Options Reference - COMPLETED
Validated against `rledger doctor list-options`.

**Fixed:**
- Removed `fiscal_year_begin` (not implemented)
- Removed `include_path` (not implemented)
- Added missing options: `account_previous_conversions`, `account_current_conversions`, `account_rounding`, `conversion_currency`, `infer_tolerance_from_cost`, `long_string_maxlines`

### Phase 7: Error Codes - COMPLETED
Validated against `crates/rustledger-validate/src/error.rs`.

**Fixed:**
- Added E3004: Transaction Has Single Posting
- Added E4004: Negative Inventory
- Added E10001: Date Out of Order
- Added E10002: Entry Dated in Future
- Added E10xxx category to table

## Files Changed

```
crates/rustledger-plugin/src/lib.rs   # Plugin count fix
docs/commands/doctor.md               # Added generate-synthetic
docs/commands/extract.md              # Fixed options
docs/commands/format.md               # Added missing options
docs/commands/price.md                # Fixed options
docs/reference/bql.md                 # Removed non-working features
docs/reference/errors.md              # Added missing error codes
docs/reference/options.md             # Fixed option list
```

## Remaining Work

- Phase 8: Guide Validation (common-queries.md, cookbook.md, etc.)
- Phase 9: Crate README Validation
- Phase 10: Link Validation
