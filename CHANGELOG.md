# Changelog

All notable changes to Herdr Colors are documented here.

## [Unreleased]

### Changed

- Adopted the canonical Herdr plugin repository structure.
- Moved theme synchronization tooling to `scripts/build/`.
- Moved the private palette generator to `libexec/`.
- Grouped the vendored theme-selection implementation with its provenance record.

- Replaced Python and shell runtime logic with Rust while keeping command names and cache paths.
- Ported palette filters and genetic selection; Python is now needed only for integration tests.
