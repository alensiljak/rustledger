#!/usr/bin/env bash
#
# Verify the `#![forbid(unsafe_code)]` invariant in every crate that declares it.
#
# This script enumerates workspace crates whose entry file contains the
# `#![forbid(unsafe_code)]` directive, then grep-searches each crate's source
# tree for `unsafe` blocks / items. If any are found, it prints a clear
# pointed error message that identifies both the offending file and the
# forbid directive the offender is violating, then exits non-zero.
#
# Why this check exists
# ---------------------
#
# `cargo check` already enforces the forbid-unsafe-code invariant as a
# compile error, so in theory this script is redundant with the existing
# `cargo check` CI job. In practice, three things made a dedicated check
# worth adding (see PR #769 for the proximate trigger):
#
# 1. When a PR adds `unsafe { ... }` to a forbid-unsafe-code crate, the
#    `cargo check` failure reads "usage of an unsafe block" without
#    mentioning the forbid directive the author is violating. Reviewers
#    had to hunt for the context. This script prints the forbid line
#    verbatim so the violation is obvious at a glance.
#
# 2. It runs in milliseconds (grep over Rust source) vs ~30 seconds for a
#    cold `cargo check`. Failing early on obvious invariant violations
#    reduces CI latency when an AI-generated PR is submitted without
#    being run through `cargo check` locally.
#
# 3. It gives the check a named CI status ("Unsafe Invariant") instead of
#    burying the failure inside the generic "Check" job's 500-line log.
#
# Exit codes
# ----------
#
#   0  all forbid-unsafe-code crates are clean
#   1  one or more crates contain `unsafe` blocks/items
#   2  invocation error (no crates found, filesystem error, etc.)
#
# Usage
# -----
#
#   ./scripts/check-unsafe-invariant.sh
#
# Runs from the repo root. No arguments, no flags, no environment variables.

set -euo pipefail

# Resolve repo root. Works whether this is invoked from the repo root,
# from a subdirectory, or by CI which checks out to an arbitrary path.
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
    echo "error: not inside a git repository" >&2
    exit 2
}
cd "$REPO_ROOT"

# Enumerate crates with `#![forbid(unsafe_code)]`. We look only at the
# crate entry file (src/lib.rs or src/main.rs) because `forbid` must be
# at the crate root to apply crate-wide.
#
# Binary-only crates use src/main.rs; library crates use src/lib.rs.
# Workspace crates live under `crates/`.
FORBID_CRATES=()
while IFS= read -r entry_file; do
    # `[[:space:]]*` is POSIX-portable; `\s*` is a PCRE escape that GNU
    # grep accepts as an extension but BSD/macOS grep treats as literal
    # `s`. Using the POSIX character class ensures the script works in
    # every CI environment and on every developer's machine.
    if grep -q '^[[:space:]]*#!\[forbid(unsafe_code)\]' "$entry_file"; then
        # Strip src/lib.rs or src/main.rs to get the crate dir
        crate_dir="${entry_file%/src/lib.rs}"
        crate_dir="${crate_dir%/src/main.rs}"
        FORBID_CRATES+=("$crate_dir")
    fi
done < <(find crates -type f \( -name lib.rs -o -name main.rs \) -path '*/src/*' 2>/dev/null)

if [[ ${#FORBID_CRATES[@]} -eq 0 ]]; then
    echo "error: found no crates with #![forbid(unsafe_code)] — is the workspace layout correct?" >&2
    exit 2
fi

echo "Checking ${#FORBID_CRATES[@]} forbid-unsafe-code crate(s)..."

# Patterns that indicate the use or declaration of `unsafe` code.
# We match:
#   - `unsafe {`     (unsafe block)
#   - `unsafe fn`    (unsafe function declaration)
#   - `unsafe impl`  (unsafe trait impl)
#   - `unsafe trait` (unsafe trait declaration)
# and anywhere the keyword `unsafe` appears as a whole word followed by
# any of the above sigils. Conservative: may catch a comment containing
# "unsafe {" but that's vanishingly rare and a false positive is cheap
# to resolve (rename the comment) vs letting a real violation through.
#
# Deliberately NOT matched: `unsafe_code` as a substring (the forbid
# directive itself), and string literals containing "unsafe" (rare in
# rustledger and cheap to suppress locally with `allow(...)` if needed).
#
# Rationale for not using rustc/clippy as the grep backend: we want
# this check to run in milliseconds without compiling anything. rustc's
# own error for the violation is perfectly good; this script exists to
# surface it faster and with better context.

VIOLATIONS_FOUND=0

for crate_dir in "${FORBID_CRATES[@]}"; do
    crate_name="${crate_dir#crates/}"

    # Find the forbid directive's location for the error message.
    # POSIX `[[:space:]]*` instead of PCRE `\s*` for portability — see
    # the comment at the enumeration loop above.
    forbid_location=""
    for entry in "$crate_dir/src/lib.rs" "$crate_dir/src/main.rs"; do
        if [[ -f "$entry" ]]; then
            line_num="$(grep -n '^[[:space:]]*#!\[forbid(unsafe_code)\]' "$entry" | head -n1 | cut -d: -f1 || true)"
            if [[ -n "$line_num" ]]; then
                forbid_location="$entry:$line_num"
                break
            fi
        fi
    done

    # Search the crate's `src/` tree for new unsafe usage. This covers
    # the library crate itself and any `#[cfg(test)]` modules inside
    # `src/`, because those compile under the same crate attributes
    # and therefore inherit the `#![forbid(unsafe_code)]` directive.
    #
    # What is NOT scanned: `tests/` (integration tests) and `benches/`
    # (benchmarks). Each file under those directories is a separate
    # crate target that does NOT inherit `src/lib.rs`'s crate-root
    # attributes. If you want similar protection for integration
    # tests, each test file needs its own `#![forbid(unsafe_code)]`.
    #
    # The regex has two alternatives because the `unsafe` + keyword
    # whitespace rules differ:
    #   - `unsafe [fn|impl|trait]` requires at least one whitespace
    #     character between `unsafe` and the keyword. Rust's lexer
    #     would merge `unsafefn` into a single identifier, so the
    #     syntactically-valid form always has whitespace.
    #   - `unsafe {` can have zero whitespace: `unsafe{ body }` is
    #     valid Rust (rustfmt will insert the space, but the raw form
    #     is legal). So we match `[[:space:]]*\{` for this case to
    #     catch `unsafe{}` alongside `unsafe {}`.
    #
    # Known limitation: a comment between `unsafe` and the following
    # token (`unsafe /* c */ {}`) is not matched. This is vanishingly
    # rare in practice and adding support would complicate the regex
    # without meaningful coverage gain.
    matches="$(grep -rn --include='*.rs' \
        -E '(^|[^[:alnum:]_])unsafe([[:space:]]+(fn|impl|trait)|[[:space:]]*\{)' \
        "$crate_dir/src" 2>/dev/null || true)"

    if [[ -n "$matches" ]]; then
        VIOLATIONS_FOUND=1
        echo
        echo "============================================================"
        echo "FORBID_UNSAFE_CODE VIOLATION in crate: $crate_name"
        echo "============================================================"
        echo "Directive: #![forbid(unsafe_code)] at $forbid_location"
        echo
        echo "Offending lines:"
        echo "$matches" | sed 's/^/    /'
        echo
        echo "Either:"
        echo "  1. Remove the unsafe block/item and use a safe alternative, or"
        echo "  2. If unsafe is genuinely required, remove the"
        echo "     #![forbid(unsafe_code)] directive from $forbid_location"
        echo "     and justify the change in the PR description."
        echo "     (Reviewers will treat removal of a forbid directive as"
        echo "     a significant change requiring extra scrutiny.)"
        echo "============================================================"
    fi
done

if [[ $VIOLATIONS_FOUND -ne 0 ]]; then
    echo
    echo "error: one or more forbid-unsafe-code invariants were violated" >&2
    exit 1
fi

echo "OK: all ${#FORBID_CRATES[@]} forbid-unsafe-code crates are clean"
exit 0
