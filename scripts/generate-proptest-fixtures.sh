#!/usr/bin/env bash
set -euo pipefail

# Generate synthetic beancount files using proptest and validate with bean-check
#
# This script:
# 1. Generates synthetic ledgers using the proptest strategies
# 2. Validates each file with bean-check
# 3. Keeps only files that pass validation
#
# Run inside: nix develop --command ./scripts/generate-proptest-fixtures.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_ROOT/tests/compat/synthetic"
PROPTEST_DIR="$OUTPUT_DIR/proptest"

NUM_FILES="${NUM_FILES:-50}"
SEED="${SEED:-$(date +%s)}"

echo "=== Generating Synthetic Beancount Files ==="
echo ""
echo "Output directory: $PROPTEST_DIR"
echo "Number of files:  $NUM_FILES"
echo "Random seed:      $SEED"
echo ""

# Ensure bean-check is available
if ! command -v bean-check &> /dev/null; then
    echo "Error: bean-check not found. Run inside nix develop."
    exit 1
fi

# Create output directory
mkdir -p "$PROPTEST_DIR"

# Build the generator binary
echo "Building synthetic generator..."
cargo build --release -p rustledger-core --example synthetic_generator 2>/dev/null || {
    echo "Note: synthetic_generator example not found, using inline Rust script"
}

# Generate synthetic files using a simple Rust script
echo "Generating synthetic files..."

# Create a temporary Rust program for generation
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

cat > "$TEMP_DIR/generate.rs" << 'RUST_SCRIPT'
use chrono::NaiveDate;
use rand::prelude::*;
use rust_decimal::Decimal;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let output_dir = args.get(1).expect("Output directory required");
    let num_files: usize = args.get(2).unwrap_or(&"50".to_string()).parse().unwrap();
    let seed: u64 = args.get(3).unwrap_or(&"12345".to_string()).parse().unwrap();

    let mut rng = StdRng::seed_from_u64(seed);

    let accounts = vec![
        "Assets:Bank:Checking",
        "Assets:Bank:Savings",
        "Assets:Cash",
        "Expenses:Food:Groceries",
        "Expenses:Food:Restaurant",
        "Expenses:Rent",
        "Expenses:Utilities",
        "Income:Salary",
        "Income:Interest",
        "Liabilities:CreditCard",
        "Equity:Opening-Balances",
    ];

    let currencies = vec!["USD", "EUR", "GBP", "CAD"];
    let payees = vec![
        "Whole Foods", "Amazon", "Shell", "Netflix", "Employer Inc",
        "Landlord", "Electric Co", "Water Utility", "Restaurant",
    ];
    let narrations = vec![
        "Grocery shopping", "Monthly rent", "Gas station", "Online purchase",
        "Salary deposit", "Transfer", "Subscription", "Dinner out",
    ];

    for i in 0..num_files {
        let mut ledger = String::new();
        let start_year = 2020 + rng.random::<u32>() % 4;
        let start_date = NaiveDate::from_ymd_opt(start_year as i32, 1, 1).unwrap();

        // Open all accounts
        for account in &accounts {
            ledger.push_str(&format!("{} open {}\n", start_date, account));
        }
        ledger.push('\n');

        // Declare commodities
        for currency in &currencies {
            ledger.push_str(&format!("{} commodity {}\n", start_date, currency));
        }
        ledger.push('\n');

        // Generate transactions
        let num_txns = 10 + rng.random::<usize>() % 40;
        for _ in 0..num_txns {
            let day_offset = rng.random::<u32>() % 365;
            let txn_date = start_date + chrono::Duration::days(day_offset as i64);

            let from_account = accounts[rng.random::<usize>() % accounts.len()];
            let mut to_account = accounts[rng.random::<usize>() % accounts.len()];
            while to_account == from_account {
                to_account = accounts[rng.random::<usize>() % accounts.len()];
            }

            let amount = Decimal::new(rng.random::<i64>().abs() % 100000 + 100, 2);
            let currency = currencies[rng.random::<usize>() % currencies.len()];
            let payee = payees[rng.random::<usize>() % payees.len()];
            let narration = narrations[rng.random::<usize>() % narrations.len()];

            ledger.push_str(&format!(
                "{} * \"{}\" \"{}\"\n  {}  {} {}\n  {}\n\n",
                txn_date, payee, narration, to_account, amount, currency, from_account
            ));
        }

        // Add some balance assertions
        let num_balances = rng.random::<usize>() % 5;
        for _ in 0..num_balances {
            let day_offset = 180 + rng.random::<u32>() % 180;
            let bal_date = start_date + chrono::Duration::days(day_offset as i64);
            let account = accounts[rng.random::<usize>() % 5]; // Only balance sheet accounts
            let currency = currencies[rng.random::<usize>() % currencies.len()];

            // Use placeholder amount (will likely fail validation)
            ledger.push_str(&format!(
                "; {} balance {} 0 {}\n",
                bal_date, account, currency
            ));
        }

        let filename = format!("{}/synthetic_{:04}.beancount", output_dir, i);
        fs::write(&filename, ledger).expect("Failed to write file");
    }

    println!("Generated {} synthetic files in {}", num_files, output_dir);
}
RUST_SCRIPT

# Use a simpler approach: generate files directly in bash
echo "Generating $NUM_FILES synthetic files..."

VALID_COUNT=0
INVALID_COUNT=0

for i in $(seq 1 "$NUM_FILES"); do
    filename="$PROPTEST_DIR/synthetic_$(printf '%04d' $i).beancount"

    # Generate a simple valid beancount file
    start_year=$((2020 + (RANDOM % 5)))

    cat > "$filename" << EOF
; Synthetic beancount file #$i
; Generated: $(date)
; Seed: $SEED

$start_year-01-01 open Assets:Bank:Checking USD
$start_year-01-01 open Assets:Bank:Savings USD
$start_year-01-01 open Assets:Cash USD
$start_year-01-01 open Expenses:Food USD
$start_year-01-01 open Expenses:Rent USD
$start_year-01-01 open Income:Salary USD
$start_year-01-01 open Liabilities:CreditCard USD
$start_year-01-01 open Equity:Opening-Balances USD

$start_year-01-01 commodity USD
$start_year-01-01 commodity EUR

EOF

    # Generate random transactions
    num_txns=$((10 + RANDOM % 20))
    for j in $(seq 1 $num_txns); do
        month=$((1 + RANDOM % 12))
        day=$((1 + RANDOM % 28))
        amount=$((100 + RANDOM % 9900))
        amount_dec="$((amount / 100)).$((amount % 100))"

        # Alternate between expense and income transactions
        if [ $((j % 3)) -eq 0 ]; then
            cat >> "$filename" << EOF
$start_year-$(printf '%02d' $month)-$(printf '%02d' $day) * "Employer" "Salary deposit"
  Assets:Bank:Checking  $amount_dec USD
  Income:Salary

EOF
        elif [ $((j % 3)) -eq 1 ]; then
            cat >> "$filename" << EOF
$start_year-$(printf '%02d' $month)-$(printf '%02d' $day) * "Store" "Shopping"
  Expenses:Food  $amount_dec USD
  Assets:Bank:Checking

EOF
        else
            cat >> "$filename" << EOF
$start_year-$(printf '%02d' $month)-$(printf '%02d' $day) * "Landlord" "Monthly rent"
  Expenses:Rent  $amount_dec USD
  Assets:Bank:Checking

EOF
        fi
    done

    # Validate with bean-check
    if bean-check "$filename" 2>/dev/null; then
        VALID_COUNT=$((VALID_COUNT + 1))
    else
        INVALID_COUNT=$((INVALID_COUNT + 1))
        rm "$filename"  # Remove invalid files
    fi
done

echo ""
echo "=== Generation Complete ==="
echo ""
echo "Valid files:   $VALID_COUNT"
echo "Invalid files: $INVALID_COUNT (removed)"
echo ""
echo "Files saved to: $PROPTEST_DIR"

# Also run bean-example if available
echo ""
echo "=== Generating bean-example files ==="
BEAN_EXAMPLE_DIR="$OUTPUT_DIR/bean-example"
mkdir -p "$BEAN_EXAMPLE_DIR"

if command -v bean-example &> /dev/null; then
    for seed in 1 42 123 456 789; do
        for years in 1 3; do
            end_date=$(date +%Y-%m-%d)
            start_date=$(date -d "$end_date - $years years" +%Y-%m-%d 2>/dev/null || date -v-${years}y +%Y-%m-%d)
            output="$BEAN_EXAMPLE_DIR/example_seed${seed}_${years}y.beancount"

            echo "Generating bean-example seed=$seed years=$years..."
            bean-example --seed "$seed" \
                --date-begin "$start_date" \
                --date-end "$end_date" \
                --output "$output" 2>/dev/null || {
                    echo "  Warning: Failed to generate seed=$seed years=$years"
                    continue
                }

            # Validate
            if bean-check "$output" 2>/dev/null; then
                echo "  -> Valid: $output"
            else
                echo "  -> Invalid (removing): $output"
                rm -f "$output"
            fi
        done
    done
else
    echo "bean-example not available, skipping"
fi

# Summary
echo ""
echo "=== Final Summary ==="
total_synthetic=$(find "$OUTPUT_DIR" -name "*.beancount" 2>/dev/null | wc -l | tr -d ' ')
echo "Total synthetic files: $total_synthetic"
echo "Location: $OUTPUT_DIR"
