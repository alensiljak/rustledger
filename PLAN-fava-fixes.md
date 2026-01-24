# Rustledger Fixes for Fava Integration

## Current State Analysis

| Issue | Status | Notes |
|-------|--------|-------|
| Cost per-unit from total | ✅ Already implemented | `CostSpec::resolve()` handles `{{1500 USD}}` |
| Cost date filling | ✅ Already implemented | `book.rs` fills missing dates with txn date |
| BQL case insensitivity | ✅ Fixed | `kw()` function now uses case-insensitive matching |
| BQL FROM `<filter>` | ✅ Fixed | Fixed to handle Number literal for year/month comparisons |
| BQL columns | ✅ Implemented | `cost_number`, `cost_currency`, `cost_date`, `cost_label`, `has_cost` |
| BQL meta access | ✅ Implemented | `META()`, `ENTRY_META()`, `POSTING_META()` functions |
| Entry hash | ✅ Implemented | SHA256 hash in FFI meta output |
| `entry` column | ✅ Implemented | Returns parent transaction as structured object |
| `meta` column | ✅ Implemented | Returns all metadata merged (posting overrides txn) |
| JOURNAL command | ✅ Fixed | Account pattern now optional |
| WASI stdin issue | ✅ Fixed | Added file-based commands (`*-file`) |
| format_entry | ✅ Implemented | `format` / `format-file` commands |
| is_encrypted_file | ✅ Implemented | `is-encrypted` command |
| get_account_type | ✅ Implemented | `get-account-type` command |
| clamp_opt | ✅ Implemented | `clamp` / `clamp-file` commands |
| Type constants | ✅ Implemented | `types` command (ALL_DIRECTIVES, Booking, MISSING) |

---

## Completed Fixes

### Phase 1: BQL Case Insensitivity ✅

**File:** `crates/rustledger-query/src/parser.rs`

**Solution:** Changed `kw()` function to use case-insensitive matching:

```rust
fn kw<'a>(keyword: &'static str) -> impl Parser<'a, ParserInput<'a>, (), ParserExtra<'a>> + Clone {
    text::ident()
        .try_map(move |s: &str, span| {
            if s.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(Rich::custom(span, format!("expected keyword '{keyword}'")))
            }
        })
}
```

**Test:** `select * from year = 2024` now works same as `SELECT * FROM year = 2024`

---

### Phase 2: BQL FROM Filter Clause ✅

**File:** `crates/rustledger-query/src/executor.rs`

**Issue:** `evaluate_from_filter()` only handled `Literal::Integer` but parser produces `Literal::Number` (Decimal).

**Solution:** Updated to handle both Integer and Number for year/month comparisons:

```rust
let year_val = match lit {
    Literal::Integer(n) => Some(*n as i32),
    Literal::Number(n) => n.to_string().parse::<i32>().ok(),
    _ => None,
};
```

---

### Phase 3: BQL Cost Columns ✅

**File:** `crates/rustledger-query/src/executor.rs`

Added new columns in `evaluate_column()`:

- `cost_number` - Per-unit cost number
- `cost_currency` - Cost currency
- `cost_date` - Cost lot date
- `cost_label` - Cost lot label
- `has_cost` - Boolean indicating if posting has cost

Also added to completions in `crates/rustledger-query/src/completions.rs`.

---

### Phase 4: BQL Meta Access ✅

**File:** `crates/rustledger-query/src/executor.rs`

Added three metadata access functions:

- `META(key)` - Check posting meta first, then transaction meta
- `ENTRY_META(key)` - Only transaction metadata
- `POSTING_META(key)` - Only posting metadata

Handles all MetaValue types (String, Number, Bool, Date, etc.).

---

### Phase 5: FFI Entry Hash ✅

**File:** `crates/rustledger-ffi-py/src/main.rs`

Added SHA256 hash computation for each directive:

- Hash is computed from canonical representation (type + core fields)
- Added `sha2` crate dependency
- Hash included in `meta.hash` field of all directive JSON output

---

## Additional Features (Just Implemented)

### Phase 6: `entry` and `meta` columns

**File:** `crates/rustledger-query/src/executor.rs`

Added `Value::Object` variant (using `BTreeMap<String, Value>`) to support structured data.

**`entry` column:** Returns the parent transaction as a structured object with fields:
- `date`, `flag`, `payee`, `narration`, `tags`, `links`, `meta`

**`meta` column:** Returns all metadata merged (posting meta overrides transaction meta).

**Files modified:**
- `crates/rustledger-query/src/executor.rs` - `Value::Object`, `entry`, `meta` columns
- `crates/rustledger-query/src/completions.rs` - Column completions
- `crates/rustledger/src/cmd/query.rs` - CLI formatting
- `crates/rustledger-ffi-py/src/main.rs` - FFI JSON conversion

Note: `JOURNAL` command IS supported (e.g., `JOURNAL "Assets:Bank" AT cost`).

---

### Phase 7: JOURNAL Parser Fix ✅

**File:** `crates/rustledger-query/src/parser.rs`

**Issue:** JOURNAL command failed with "parse error at position 0: unexpected end of input" because the account pattern was mandatory.

**Solution:** Made account pattern optional:

```rust
kw("JOURNAL")
    .ignore_then(
        ws1()
            .ignore_then(string_literal())
            .or_not(),  // Account pattern is now optional
    )
```

---

### Phase 8: WASI File-Based Commands ✅

**File:** `crates/rustledger-ffi-py/src/main.rs`

**Issue:** Stdin piping doesn't work reliably in wasmtime 40.0+ with WASI Preview2.

**Solution:** Added file-based commands as alternatives:

- `load-file <path>` - Load from file
- `validate-file <path>` - Validate from file
- `query-file <path> <bql>` - Query from file
- `batch-file <path> <bql>...` - Batch queries from file

---

## Testing

All tests pass:

```bash
cargo test --all
```

### WASI Module Usage

**Important:** Use the release build and increase stack size for queries:

```bash
# Build release WASM
nix develop .#wasm -c cargo build --package rustledger-ffi-py --target wasm32-wasip1 --release

# Use file-based commands (recommended)
wasmtime -W max-wasm-stack=8388608 --dir=. rustledger-ffi-py.wasm load-file ledger.beancount
wasmtime -W max-wasm-stack=8388608 --dir=. rustledger-ffi-py.wasm query-file ledger.beancount "JOURNAL"
wasmtime -W max-wasm-stack=8388608 --dir=. rustledger-ffi-py.wasm query-file ledger.beancount "SELECT date, account FROM year = 2024"
```

### WASI Notes

1. **Release build required**: Debug builds crash due to larger stack frames with chumsky parser
2. **Stack size**: Use `-W max-wasm-stack=8388608` (8MB) for queries
3. **File-based commands**: Use `*-file` commands instead of stdin piping
4. **Directory access**: Use `--dir=.` to grant file system access

---

### Phase 9: Fava Integration APIs ✅

**File:** `crates/rustledger-ffi-py/src/main.rs`

Added commands required for full Fava integration without beancount dependency:

#### `format` / `format-file`
Formats entries back to beancount source syntax:
```bash
rustledger-ffi-py format-file ledger.beancount
# Returns: {"formatted": "2024-01-01 open Assets:Bank USD\n...", "errors": []}
```

#### `is-encrypted`
Detects GPG-encrypted files:
```bash
rustledger-ffi-py is-encrypted ledger.beancount.gpg
# Returns: {"encrypted": true, "reason": "file extension"}
```
Checks: `.gpg`/`.asc` extension, ASCII armor header, GPG binary header.

#### `get-account-type`
Extracts account type from account name:
```bash
rustledger-ffi-py get-account-type "Assets:Bank:Checking"
# Returns: {"account": "Assets:Bank:Checking", "account_type": "Assets"}
```

#### `clamp` / `clamp-file`
Filters entries by date range with proper accounting semantics:
```bash
rustledger-ffi-py clamp-file ledger.beancount 2024-01-01 2024-12-31
# Returns: {"entries": [...], "opening_balances": [...], "errors": []}
```
- Entries within date range
- Open/Commodity directives from before begin date
- Opening balances computed from transactions before begin date

#### `types`
Returns type constants for Fava compatibility:
```bash
rustledger-ffi-py types
# Returns:
# {
#   "all_directives": ["transaction", "balance", "open", ...],
#   "booking_methods": ["STRICT", "FIFO", "LIFO", ...],
#   "missing": {"description": "...", "json_representation": "null or {...}"},
#   "account_types": ["Assets", "Liabilities", "Equity", "Income", "Expenses"]
# }
```
