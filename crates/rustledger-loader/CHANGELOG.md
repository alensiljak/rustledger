# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.1](https://github.com/rustledger/rustledger/compare/v0.12.0...v0.12.1) - 2026-04-21

### Bug Fixes

- eliminate duplication, fix file reading and encrypted file handling
- preserve include order and surface I/O errors in parallel path
- use FileSystem trait for parallel reads instead of raw std::fs
- read only 1024 bytes for .asc encryption detection
- address Copilot review — tighten validator, fix Options, update docs
- execute WASM plugins during file loading
- handle Python and unknown plugins in loader, add tests
- inline format variables for clippy
- address review comments on plugin execution
- clippy (collapsible if, inline format, case-insensitive ext)
- case-insensitive .py extension, fix phase type in test
- make WASM test resilient to coverage instrumentation
- order augmentations before reductions on same date
- address review comments on sort ordering
- address review comments on plugin consolidation
- restore specific error codes and suggest_module_path in run_plugins()

### Performance

- parallel file loading for multi-file ledgers
- avoid cloning directive list for native plugin execution

### Refactoring

- consolidate plugin execution into run_plugins()

### Testing

- add regression test for parallel multi-file loading

## [0.12.0](https://github.com/rustledger/rustledger/compare/v0.11.0...v0.12.0) - 2026-04-11

### Bug Fixes

- match beancount error message wording (4 cases)
- exclude price directives from display precision tracking
- exclude posting price annotations from display precision tracking
- *(booking)* apply per-account methods across all consumers
- align include-cycle error wording with Python beancount

### Refactoring

- extract reintern_directive helper for plain and Spanned usage

## [0.11.0](https://github.com/rustledger/rustledger/compare/v0.10.1...v0.11.0) - 2026-04-02

### Bug Fixes

- *(loader)* support glob patterns in VirtualFileSystem includes
- address review comments on VFS glob support

### Features

- *(wasm)* add multi-file API for include resolution

### Testing

- add VFS glob test for ./ prefix normalization

## [0.10.1](https://github.com/rustledger/rustledger/compare/v0.10.0...v0.10.1) - 2026-03-12

### Bug Fixes

- address PR review feedback on documentation audit

### Documentation

- comprehensive documentation audit and corrections

## [0.10.0](https://github.com/rustledger/rustledger/compare/v0.9.0...v0.10.0) - 2026-02-18

### Bug Fixes

- address PR review comments

### Features

- *(ci)* add per-platform status badges to README

## [0.8.8](https://github.com/rustledger/rustledger/compare/v0.8.7...v0.8.8) - 2026-02-14

### Bug Fixes

- *(docs)* address Copilot review feedback on PR #351

### Documentation

- comprehensive documentation overhaul

## [0.8.4](https://github.com/rustledger/rustledger/compare/v0.8.3...v0.8.4) - 2026-02-06

### Performance

- optimize hash maps, add parallel query execution, improve test coverage

## [0.8.2](https://github.com/rustledger/rustledger/compare/v0.8.1...v0.8.2) - 2026-02-02

### Features

- *(loader)* add central orchestration API matching Python's loader.load_file()
- *(plugin)* enable Python plugin execution via WASM sandbox

### Testing

- add coverage for Python plugin execution

### Style

- address PR review suggestions

## [0.8.0](https://github.com/rustledger/rustledger/releases/tag/v0.8.0) - 2026-01-28

### Bug Fixes

- *(balance)* use BigDecimal for precise residual checking and fix tolerance defaults
- *(check)* don't cache files with option validation warnings

### Style

- rustfmt
- fix clippy warnings after MSRV alignment

## [0.7.2](https://github.com/rustledger/rustledger/compare/v0.7.1...v0.7.2) - 2026-01-25

### Performance

- optimize booking posting expansion and loader file handling

## [0.7.0](https://github.com/rustledger/rustledger/releases/tag/v0.7.0) - 2026-01-25

### Bug Fixes

- *(loader)* use WASI-compatible path normalization

### Refactoring

- consolidate rledger-\* binaries into single rledger binary

## [0.6.0](https://github.com/rustledger/rustledger/releases/tag/v0.6.0) - 2026-01-23

### Bug Fixes

- *(ci)* resolve cargo vet and codecov issues
- address Copilot review suggestions
- achieve 100% BQL compatibility with Python beancount
- address PR review comments and clippy warnings

### Documentation

- update install options in README

### Features

- add infrastructure for validation error line numbers
- add DisplayContext for consistent number formatting
- comprehensive benchmark infrastructure overhaul
- achieve 100% BQL query compatibility with Python beancount
- enhance compatibility CI with comprehensive testing

## [0.5.2](https://github.com/rustledger/rustledger/compare/v0.5.1...v0.5.2) - 2026-01-20

## [0.5.1](https://github.com/rustledger/rustledger/releases/tag/v0.5.1) - 2026-01-20

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85

## [0.5.0](https://github.com/rustledger/rustledger/compare/v0.4.0...v0.5.0) - 2026-01-19

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
