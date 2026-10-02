# Changelog

All notable changes to Herdr Colors are documented here.

## [Unreleased]

### Changed

- Adopted the canonical Herdr plugin repository structure.
- Moved build/install tooling to `scripts/build/` and kept compatibility
  launchers in `bin/`.
- Replaced Python and shell runtime logic with Rust under `src/` while keeping
  command names and cache paths.
- Ported palette filters and genetic selection; Python is now needed only for integration tests.
