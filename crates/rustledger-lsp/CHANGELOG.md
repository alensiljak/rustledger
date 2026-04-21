# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.1](https://github.com/rustledger/rustledger/compare/v0.12.0...v0.12.1) - 2026-04-21

### Bug Fixes

- handle deserialization failures in async dispatch
- update LSP test for Unicode account support
- resolve all Rust 1.95 clippy lints across workspace
- address review comments on sort ordering
- also emit option warnings in single-file mode

### Features

- implement background request dispatch for expensive LSP operations
- expose option warnings (E7001–E7006) in LSP and WASM

### Refactoring

- remove duplicated code and dead code across codebase
- third pass — remove unused error variant, constant, and field
- improve LSP background dispatch architecture
- *(core)* replace chrono with jiff in rustledger-core
- migrate remaining crates from chrono to jiff

## [0.12.0](https://github.com/rustledger/rustledger/compare/v0.11.0...v0.12.0) - 2026-04-11

### Bug Fixes

- *(parser)* reject 7 invalid inputs per beancount v3 spec
- address Copilot review feedback on parser strict
- *(lsp)* prevent panic on multi-byte UTF-8 characters in completions
- proper UTF-16 conversion and emoji test per Copilot review
- *(lsp)* use lexer tokens for semantic highlighting
- use UTF-16 code units for semantic token positions
- *(booking)* apply per-account methods across all consumers
- overlay in-memory buffer on stale ledger snapshot for LSP diagnostics
- avoid double-clone of full_directives on overlay path ([#758](https://github.com/rustledger/rustledger/pull/758))
- overlay all open buffers when publishing LSP diagnostics ([#764](https://github.com/rustledger/rustledger/pull/764))

### Documentation

- update VS Code setup to use rustledger extension
- clarify char_offset_to_byte handles code points not UTF-16 units
- drop intra-doc link from public fn to private helper

### Features

- *(vscode)* distribute via GitHub Releases instead of marketplace

### Performance

- O(log n) line lookup and O(1) source matching
- skip out-of-range tokens in range request

### Refactoring

- *(core)* deduplicate extract_accounts/currencies/payees

### Testing

- add emoji UTF-16 position assertions for semantic tokens
- add end-to-end all_diagnostics regression test and helper invariant

## [0.11.0](https://github.com/rustledger/rustledger/compare/v0.10.0...v0.11.0) - 2026-03-12

### Bug Fixes

- use full ledger for validation diagnostics in multi-file projects (#470)

### Testing

- add regression test for issue #470

## [0.10.0](https://github.com/rustledger/rustledger/compare/v0.9.0...v0.10.0) - 2026-02-18

### Bug Fixes

- *(docs)* address Copilot review feedback on PR #351
- *(lsp)* address PR review comments

### Documentation

- remove unneeded Helix config fields
- update install instructions for homebrew-core
- comprehensive documentation overhaul
- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.9.0](https://github.com/rustledger/rustledger/releases/tag/v0.9.0) - 2026-02-17

### Bug Fixes

- *(docs)* address Copilot review feedback on PR #351
- *(lsp)* address PR review comments

### Documentation

- update install instructions for homebrew-core
- comprehensive documentation overhaul
- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.8](https://github.com/rustledger/rustledger/compare/v0.8.7...v0.8.8) - 2026-02-14

### Bug Fixes

- *(docs)* address Copilot review feedback on PR #351
- *(lsp)* address PR review comments

### Documentation

- comprehensive documentation overhaul
- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.7](https://github.com/rustledger/rustledger/compare/v0.8.6...v0.8.7) - 2026-02-14

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.6](https://github.com/rustledger/rustledger/compare/v0.8.5...v0.8.6) - 2026-02-09

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.5](https://github.com/rustledger/rustledger/compare/v0.8.4...v0.8.5) - 2026-02-08

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.4](https://github.com/rustledger/rustledger/compare/v0.8.3...v0.8.4) - 2026-02-06

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.3](https://github.com/rustledger/rustledger/compare/v0.8.2...v0.8.3) - 2026-02-05

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.2](https://github.com/rustledger/rustledger/compare/v0.8.1...v0.8.2) - 2026-02-02

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.1](https://github.com/rustledger/rustledger/compare/v0.8.0...v0.8.1) - 2026-01-29

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.8.0](https://github.com/rustledger/rustledger/releases/tag/v0.8.0) - 2026-01-28

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

### Style

- fix clippy warnings after MSRV alignment

## [0.7.5](https://github.com/rustledger/rustledger/compare/v0.7.4...v0.7.5) - 2026-01-26

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.7.4](https://github.com/rustledger/rustledger/compare/v0.7.3...v0.7.4) - 2026-01-26

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.7.2](https://github.com/rustledger/rustledger/compare/v0.7.1...v0.7.2) - 2026-01-25

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.7.1](https://github.com/rustledger/rustledger/compare/v0.7.0...v0.7.1) - 2026-01-25

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.7.0](https://github.com/rustledger/rustledger/releases/tag/v0.7.0) - 2026-01-25

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.6.0](https://github.com/rustledger/rustledger/releases/tag/v0.6.0) - 2026-01-23

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.5.2](https://github.com/rustledger/rustledger/compare/v0.5.1...v0.5.2) - 2026-01-20

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.5.1](https://github.com/rustledger/rustledger/compare/v0.5.0...v0.5.1) - 2026-01-19

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.5.0](https://github.com/rustledger/rustledger/compare/v0.4.0...v0.5.0) - 2026-01-19

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- \[**breaking**\] upgrade to Rust 2024 edition and MSRV 1.85
- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication

## [0.4.0](https://github.com/rustledger/rustledger/releases/tag/v0.4.0) - 2026-01-18

### Bug Fixes

- *(lsp)* address PR review comments

### Documentation

- add comprehensive PR review policy

### Features

- *(lsp)* add resolve handlers and file watching
- *(lsp)* add execute command and completion resolve
- *(lsp)* add call hierarchy and signature help
- *(lsp)* add code lens, document color, goto declaration
- *(lsp)* add document highlight, linked editing, on-type formatting
- *(lsp)* add type hierarchy for account navigation
- *(lsp)* add find references for accounts, currencies, payees
- *(lsp)* add range formatting, document links, inlay hints, selection range
- *(lsp)* add workspace symbols, rename, formatting, folding
- *(lsp)* add code actions for quick fixes
- *(lsp)* add semantic tokens for syntax highlighting
- *(lsp)* add Phase 5 - document symbols (outline view)
- *(lsp)* add Phase 4 - navigation features (definition, hover)
- *(lsp)* add Phase 3 - autocompletion support
- *(lsp)* implement Phase 1 & 2 - main loop with diagnostics
- *(lsp)* add rustledger-lsp crate skeleton (WIP)

### Performance

- *(lsp,wasm)* add caching and optimize position lookups

### Refactoring

- remove dead code and fix duplication
