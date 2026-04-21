# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.13.0](https://github.com/rustledger/rustledger/compare/v0.12.0...v0.13.0) - 2026-04-21

### Bug Fixes

- address Copilot review on price source timeout docs and test helpers
- resolve remaining Rust 1.95 clippy lints in rustledger CLI
- replace map().unwrap_or(false) with is_ok_and() in integration test
- add missing postings table columns for beancount compatibility
- weight for @@ total price, describe id type, evaluate_column parity
- add visible meta column to postings table
- address review comments on plugin execution
- remove duplicate WASM plugin execution in check command
- address review comments on plugin consolidation
- accept E8002 in Python module plugin error test
- address review — safety, portability, and test coverage

### Documentation

- fix inaccuracies found during codebase audit
- update all references to bean-* wrapper scripts

### Features

- add Nushell shell completions

### Refactoring

- remove duplicated code and dead code across codebase
- remove more dead code found in second pass
- third pass — remove unused error variant, constant, and field
- consolidate plugin execution into run_plugins()
- make bean-compat opt-in, add rledger compat install

## [0.12.0](https://github.com/rustledger/rustledger/compare/v0.11.0...v0.12.0) - 2026-04-11

### Bug Fixes

- find bean-check binary in release profile for nix builds
- add space after { in cost display, cap display precision at 8
- add space after { in cost display to match Python beanquery
- avoid per-error source clone, simplify spans, add CJK test
- *(check)* emit E1005 (invalid account name) as parse-phase diagnostic
- *(test)* inline format arg in cli_commands_test to satisfy clippy
- *(booking)* propagate errors, track per-account booking methods
- *(booking)* apply per-account methods across all consumers
- align include-cycle error wording with Python beancount

### Features

- implement price caching for rledger price command
- add pager support for report output
- add phase field to JSON diagnostics for parse/validate separation
- *(query)* per-column display context for BQL output
- migrate from archived ariadne to miette for error diagnostics

### Testing

- add unit tests for diagnostic phase field

## [0.11.0](https://github.com/rustledger/rustledger/compare/v0.10.1...v0.11.0) - 2026-04-02

### Bug Fixes

- adapt to sha2 0.11.0 API changes
- *(price)* address PR review feedback
- address PR review feedback for custom price sources
- route WASM plugins declared in beancount file to WASM runtime
- address Copilot review comments

### Documentation

- fix currency_accounts plugin description

### Features

- add filtering options to report networth
- *(bql)* support numeric and mixed-type sets in IN operator
- *(extract)* add --list-importers and filename pattern matching
- *(price)* implement custom price sources with pluggable registry

## [0.10.1](https://github.com/rustledger/rustledger/compare/v0.10.0...v0.10.1) - 2026-03-12

### Bug Fixes

- address review comments on config feature

### Documentation

- update README plugin count from 20 to 30

### Features

- add configuration file support
- *(config)* add aliases and command-specific defaults

### Testing

- add comprehensive tests for config file feature

## [0.10.0](https://github.com/rustledger/rustledger/compare/v0.9.0...v0.10.0) - 2026-02-18

### Bug Fixes

- *(tests)* handle missing rledger binary in fixture_tests.rs
- *(tests)* gracefully handle missing rledger binary in CLI tests
- *(tests)* use CARGO_BIN_EXE for CLI tests

## [0.8.8](https://github.com/rustledger/rustledger/compare/v0.8.7...v0.8.8) - 2026-02-14

### Bug Fixes

- address review feedback on comments and error messages

### Features

- *(cli)* add completions subcommand for shell completion generation

## [0.8.6](https://github.com/rustledger/rustledger/compare/v0.8.5...v0.8.6) - 2026-02-09

### Miscellaneous

- update Cargo.lock dependencies

## [0.8.4](https://github.com/rustledger/rustledger/compare/v0.8.3...v0.8.4) - 2026-02-06

### Miscellaneous

- update Cargo.lock dependencies

## [0.8.3](https://github.com/rustledger/rustledger/compare/v0.8.2...v0.8.3) - 2026-02-05

### Miscellaneous

- update Cargo.lock dependencies

## [0.8.2](https://github.com/rustledger/rustledger/compare/v0.8.1...v0.8.2) - 2026-02-02

### Bug Fixes

- address PR review feedback

### Features

- *(loader)* add central orchestration API matching Python's loader.load_file()
- *(plugin)* enable Python plugin execution via WASM sandbox

### Style

- fix clippy warnings and formatting

## [0.8.0](https://github.com/rustledger/rustledger/releases/tag/v0.8.0) - 2026-01-28

### Bug Fixes

- *(balance)* propagate tolerance options and preserve total costs/prices
- *(check)* don't cache files with option validation warnings
- *(query)* tolerate parse errors like bean-query does
- line #s

### Miscellaneous

- reorganize test fixtures and cleanup

### Style

- fix clippy warnings after MSRV alignment

## [0.7.5](https://github.com/rustledger/rustledger/compare/v0.7.4...v0.7.5) - 2026-01-26

### Miscellaneous

- update Cargo.lock dependencies

## [0.7.4](https://github.com/rustledger/rustledger/compare/v0.7.3...v0.7.4) - 2026-01-26

### Miscellaneous

- update Cargo.lock dependencies

## [0.7.2](https://github.com/rustledger/rustledger/compare/v0.7.1...v0.7.2) - 2026-01-25

### Bug Fixes

- *(booking)* sort directives by date before lot matching

### Performance

- box heavy Value variants and avoid balance prefix alloc

### Style

- apply cargo fmt

## [0.7.1](https://github.com/rustledger/rustledger/compare/v0.7.0...v0.7.1) - 2026-01-25

### Bug Fixes

- *(doctor)* convert byte offsets to line numbers in context command

## [0.7.0](https://github.com/rustledger/rustledger/releases/tag/v0.7.0) - 2026-01-25

### Bug Fixes

- *(test)* handle broken pipe gracefully in stdin test
- *(booking)* run booking before interpolation in check command

### Features

- *(synthetic)* add rledger-doctor generate-synthetic subcommand
- *(ffi-py)* add Fava integration APIs and BQL improvements
- *(bql)* add CREATE TABLE, INSERT, interval(), and SELECT FROM table

### Miscellaneous

- update remaining references to single rledger binary

### Refactoring

- consolidate rledger-\* binaries into single rledger binary
- *(cli)* split doctor.rs into command modules
- *(cli)* split report_cmd into focused modules

### Testing

- *(cli)* add comprehensive CLI command integration tests

### Style

- apply cargo fmt
- apply cargo fmt
- apply cargo fmt

## [0.6.0](https://github.com/rustledger/rustledger/releases/tag/v0.6.0) - 2026-01-23

### Bug Fixes

- add TTY detection for colored output
- address PR review comments and clippy warnings

### Documentation

- update install options in README

### Features

- *(validate)* add validate_spanned_with_options for location tracking
- add infrastructure for validation error line numbers
- add DisplayContext for consistent number formatting
- comprehensive benchmark infrastructure overhaul
- achieve 100% BQL query compatibility with Python beancount
- enhance compatibility CI with comprehensive testing
- add beancount compatibility testing framework
- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85

## [0.5.2](https://github.com/rustledger/rustledger/compare/v0.5.1...v0.5.2) - 2026-01-20

### Miscellaneous

- update Cargo.lock dependencies

## [0.5.1](https://github.com/rustledger/rustledger/compare/v0.5.0...v0.5.1) - 2026-01-19

### Miscellaneous

- update Cargo.lock dependencies

## [0.5.0](https://github.com/rustledger/rustledger/compare/v0.4.0...v0.5.0) - 2026-01-19

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
