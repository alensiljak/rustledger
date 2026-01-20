# Beancount Compatibility Report

This document describes the compatibility between rustledger and Python beancount, based on testing 816 real-world beancount files from multiple sources.

## Summary

| Metric | Value |
|--------|-------|
| Files tested | 816 |
| Check exit match | **89%** (734/816) |
| Parse behavior match | **95%** (777/816) |
| BQL query data match | **69%** (138/200) |

## Test Sources

Files were collected from:
- beancount v2/v3 official repositories
- beancount-parser-lima test suite
- fava web interface fixtures
- beangulp importer framework
- ledger2beancount converter tests
- beancount-import test data
- Community plugin repositories

## Mismatch Analysis

### Rustledger Errors (Python passes, Rust fails): 37 files

Most mismatches are due to stricter validation in rustledger:

1. **Balance validation** - Rustledger enforces transaction balancing more strictly, particularly for multi-currency transactions without explicit prices.

2. **Validation errors** - Some files that Python accepts silently produce validation errors in rustledger.

Example:
```beancount
; Python accepts this multi-currency transaction without a price
2008-04-02 * "Payment"
  Assets:Cash     440.00 CAD
  Assets:Receivable  -431.92 USD
  Assets:Cash
```

Rustledger reports: `Transaction does not balance: residual 440.00 CAD`

### Python Errors (Rust passes, Python fails): 45 files

These are expected differences where Python has features rustledger doesn't implement:

| Category | Count | Description |
|----------|-------|-------------|
| **Plugin errors** | 10 | Python plugin configuration options |
| **Option errors** | 13 | Options that Python validates strictly |
| **Push/pop errors** | 12 | pushtag/poptag validation differences |
| **Other** | 10 | Deprecated features, edge cases |

These are mostly test files specifically designed to trigger Python errors:
- `PluginProcessingMode*.beancount` - Python plugin configuration
- `PushmetaForgotten.beancount` - Missing popmeta directives
- `DeprecatedOption.beancount` - Deprecated beancount options

## Compatibility by Source

| Source | Files | Match Rate |
|--------|-------|------------|
| compat/ | 579 | 90% |
| lima-tests/ | 223 | 89% |
| examples/ | 8 | 87% |
| root | 3 | 100% |
| python-plugins/ | 3 | 66% |

## Known Differences

### 1. Multi-Currency Transactions

Python beancount allows transactions with multiple currencies without explicit conversion prices. Rustledger requires either:
- A price (`@` or `@@`) annotation
- All postings in the same currency
- Explicit balancing

**Workaround**: Add `@ 1.0 USD` or appropriate price to multi-currency transactions.

### 2. Python Plugin Loading

Rustledger does not execute Python plugins. Files using `plugin "some_python_plugin"` will:
- Parse successfully
- Report error E8001 "Plugin not found" for unknown plugins
- This matches Python beancount's behavior of failing on missing plugins

Rustledger supports 20 native plugins that match Python beancount behavior:
- `auto_accounts`, `auto_tag`, `check_closing`, `check_commodity`
- `check_drained`, `check_average_cost`, `close_tree`, `coherent_cost`
- `commodity_attr`, `currency_accounts`, `implicit_prices`, `leafonly`
- `noduplicates`, `nounused`, `onecommodity`, `pedantic`
- `sellgains`, `unique_prices`, `unrealized`, `document_discovery`

**Workaround**: Use rustledger's native plugins where available, or remove unsupported plugin directives.

### 3. Push/Pop Meta and Tag Validation

Python beancount validates that `pushtag`/`poptag` and `pushmeta`/`popmeta` directives are balanced. Rustledger's validation is less strict in some edge cases.

### 4. Deprecated Options

Python beancount reports errors for deprecated options like `plugin_processing_mode`. Rustledger ignores unknown options.

## BQL Query Compatibility

BQL (Beancount Query Language) compatibility was tested with 4 standard queries on 50 files:

| Query | Description |
|-------|-------------|
| `SELECT DISTINCT account` | List all accounts |
| `SELECT COUNT(*)` | Count transactions |
| `SELECT currency, COUNT(*) GROUP BY currency` | Currency breakdown |
| `SELECT YEAR(date), COUNT(*) GROUP BY year` | Annual counts |

**Results: 69% data match**

Differences are mainly due to:
- Output formatting (column widths, separators)
- Amount representation (trailing zeros, decimal places)
- Empty result handling

Note: The 69% match rate compares actual data values, ignoring formatting differences.

## Running Compatibility Tests

```bash
# Inside nix develop shell:

# Quick test with curated files (~100 files, committed)
./scripts/compat-test.sh tests/compat/files

# Full test suite (~800 files, downloaded)
./scripts/fetch-compat-test-files.sh  # Download first
./scripts/compat-test.sh               # Runs on tests/compat-full/

# Run BQL comparison
./scripts/compat-bql-test.sh

# Analyze results
python scripts/analyze-compat-results.py
```

## Directory Structure

```
tests/compat/                    # Curated test suite (committed)
├── README.md                    # Test documentation
├── sources.toml                 # Source documentation and licenses
└── files/                       # ~100 curated beancount files
    ├── parser/                  # Parser edge cases
    ├── validation/              # Validation scenarios
    ├── plugins/                 # Plugin configurations
    ├── real-world/              # Real-world examples
    └── edge-cases/              # Known compatibility differences

tests/compat-full/               # Full test suite (gitignored, downloaded)
├── beancount-v2/                # Official beancount v2 files
├── beancount-v3/                # Official beancount v3 files
├── parser-lima/                 # Parser conformance tests
├── fava/                        # Fava web interface tests
├── beangulp/                    # Importer framework examples
├── ledger2beancount/            # Converter tests
├── beancount-import/            # Import test data
└── community/                   # Community project files

tests/compat-results/            # Test output (gitignored)
```

## Scripts

- `scripts/fetch-compat-test-files.sh` - Downloads full test suite from GitHub
- `scripts/compat-test.sh` - Main test harness (bean-check vs rledger-check)
- `scripts/compat-bql-test.sh` - BQL query comparison
- `scripts/analyze-compat-results.py` - Results analysis and reporting

## Improving Compatibility

If you encounter a file that works with Python beancount but not rustledger:

1. Check if it uses Python plugins (expected to fail)
2. Check for multi-currency transactions without prices
3. File an issue at https://github.com/rustledger/rustledger/issues

---

*Generated: January 2026*
*Test environment: Beancount 3.2.0, rustledger 0.5.2*
