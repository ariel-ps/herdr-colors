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
- Require complete ANSI palettes and the selector's minimum simulated-deuteranopia distance.
- Added contextual command help and bounded CIE Lab lightness input.
- Added a JSON detection API for current Herdr pane, terminal, geometry, and neighbors.
- Added deterministic genetic assignment across touching panes, a JSON options API,
  and location-aware `apply`/`colorize` behavior.

### Fixed

- Validate every cached palette slot during synchronization.
- Avoid terminal escape sequences when shell output is redirected.
- Treat an empty `HERDR_BIN_PATH` as unset and relative `XDG_CACHE_HOME` as invalid.
- Reject non-UTF-8 arguments without panicking.
- Verify a pane's PID and terminal again before repainting.
- Clone themes through a temporary directory to tolerate concurrent synchronization.

### Security

- Validate pane IDs and cached OSC sequences before terminal output.
- Reject malformed or duplicate theme colors and escape unsafe theme-name characters.
