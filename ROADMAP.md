# Roadmap

This document outlines the development roadmap for rustledger.

## Current Status (v0.5.x)

rustledger is a **production-ready** Beancount-compatible accounting tool with:

- ✅ **100% compatibility** with Python beancount on 600+ test files
- ✅ **10-30x faster** than Python beancount
- ✅ Full BQL query language support (99% query compatibility)
- ✅ 20 native plugins matching Python behavior
- ✅ 7 booking methods (FIFO, LIFO, HIFO, etc.)
- ✅ Multi-platform binaries (Linux, macOS, Windows)
- ✅ WebAssembly build for browser/JS use
- ✅ Language Server Protocol (LSP) implementation

## Near-Term (v0.6.x)

### Editor Integration
- [ ] **VS Code extension**: Bundle `rledger-lsp` for zero-config editing
- [ ] **mason.nvim registry**: Easy LSP installation for Neovim users
- [ ] **Helix/Zed support**: Configuration guides and testing

### Performance
- [ ] **Parallel parsing**: Multi-threaded file loading for large ledgers
- [ ] **Incremental updates**: Only re-process changed portions of files

### Compatibility
- [ ] **Python plugin bridge**: Run Python plugins via subprocess (opt-in)
- [ ] **Fava integration**: Test and document fava compatibility

## Medium-Term (v0.7.x - v0.8.x)

### New Features
- [ ] **Web UI**: Lightweight alternative to fava (Rust + WASM)
- [ ] **Import framework**: Native importer system similar to beangulp
- [ ] **Forecasting**: Built-in transaction forecasting and projection

### Query Enhancements
- [ ] **Query caching**: Cache parsed queries and results
- [ ] **Custom functions**: User-defined BQL functions
- [ ] **Export formats**: Direct export to CSV, JSON, Parquet

### Plugin System
- [ ] **WASM plugins**: Load plugins as WebAssembly modules
- [ ] **Plugin marketplace**: Registry for community plugins

## Long-Term Vision (v1.0+)

### Ecosystem
- [ ] **PTA Spec participation**: Contribute to Plain Text Accounting standardization
- [ ] **Multi-format support**: Optional ledger/hledger syntax parsing
- [ ] **Migration tools**: Automated conversion from other PTA tools

### Enterprise Features
- [ ] **Multi-user support**: Concurrent editing with conflict resolution
- [ ] **Audit logging**: Track all changes for compliance
- [ ] **API server**: REST/GraphQL API for integrations

### Distribution
- [ ] **Homebrew core**: Get into homebrew-core (not just tap)
- [ ] **Linux packages**: .deb, .rpm, AUR, nixpkgs
- [ ] **Chocolatey**: Windows package manager

## Non-Goals

These are explicitly **not** planned:

- **GUI desktop app**: Use VS Code + extension or web UI instead
- **Cloud service**: rustledger is local-first; no hosted version planned
- **Breaking Beancount compatibility**: We follow beancount syntax strictly

## Detailed Roadmaps

For implementation details, see these focused roadmaps:

- **[Performance Roadmap](docs/PERFORMANCE_ROADMAP.md)** - Optimization phases, benchmarks, cache implementation
- **[Testing Roadmap](docs/TESTING_ROADMAP.md)** - Testing infrastructure, fuzzing, formal verification
- **[TLA+ Status](spec/tla/ROADMAP.md)** - Formal specification coverage and verification status

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute. Priority areas:

1. **Testing**: More real-world beancount files for compatibility testing
2. **Documentation**: Tutorials, guides, examples
3. **Editor plugins**: VS Code, Neovim, Emacs configurations
4. **Importers**: Bank statement importers for various institutions

## Versioning

We follow [Semantic Versioning](https://semver.org/):

- **Patch** (0.5.x): Bug fixes, performance improvements
- **Minor** (0.x.0): New features, backward-compatible
- **Major** (x.0.0): Breaking changes (none planned before 1.0)
