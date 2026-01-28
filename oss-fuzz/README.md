# OSS-Fuzz Integration

This directory contains files for integrating rustledger with [OSS-Fuzz](https://github.com/google/oss-fuzz), Google's continuous fuzzing service for open source projects.

## Files

These files should be submitted as a PR to the `google/oss-fuzz` repository under `projects/rustledger/`:

- `project.yaml` - Project metadata
- `Dockerfile` - Build environment
- `build.sh` - Build script for fuzz targets

## Submitting to OSS-Fuzz

1. Fork https://github.com/google/oss-fuzz
2. Create `projects/rustledger/` directory
3. Copy these files into that directory
4. Submit a PR

## Fuzz Targets

| Target | Crate | Description |
|--------|-------|-------------|
| `fuzz_parse` | rustledger-parser | Raw bytes → UTF-8 → parse() |
| `fuzz_parse_line` | rustledger-parser | Structured directive generation |
| `fuzz_query_parse` | rustledger-query | BQL query parser |

## Local Testing

To test the OSS-Fuzz build locally:

```bash
# Clone oss-fuzz
git clone https://github.com/google/oss-fuzz
cd oss-fuzz

# Copy rustledger project files
mkdir -p projects/rustledger
cp /path/to/rustledger/oss-fuzz/* projects/rustledger/

# Build
python infra/helper.py build_image rustledger
python infra/helper.py build_fuzzers rustledger

# Run a fuzzer
python infra/helper.py run_fuzzer rustledger fuzz_parse
```

## References

- [OSS-Fuzz documentation](https://google.github.io/oss-fuzz/)
- [Rust fuzzing guide](https://google.github.io/oss-fuzz/getting-started/new-project-guide/rust-lang/)
