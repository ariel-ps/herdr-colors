# Palette selection provenance

`src/palette.rs` ports the selection logic previously shipped in
[`vendor/theme-ga.py`](https://github.com/ariel-ps/herdr-colors/blob/06556c8322cbb8aa18889d1a40c986e8d94e91cc/vendor/theme-ga.py).
It preserves its CIE Lab conversion, deuteranopia simulation, contrast/chroma/
colorfulness filters, and genetic search defaults. Different random generators
mean that individual selected sets can differ from Python.

The Rust integration additionally requires complete ANSI palettes and returns an
error when no selected set reaches the original minimum simulated-deuteranopia
distance, instead of silently accepting the best failing set.

The original helper came from a personal dotfiles repository whose URL and
revision were not retained. The earliest recoverable copy is herdr-kit commit
`bb1d56a500865fe7993a3672ba75561bcb40db76`; it entered Herdr Colors at
`5aa43fb434dfe431799afebd5022e1254c370b66`. No independent upstream license record
was retained; the Rust port does not resolve that historical provenance gap.

Runtime libraries `serde_json` and `rand`, including their transitive dependencies,
are recorded in `Cargo.lock` and retain their package licenses. Downloaded Kitty
themes retain their upstream terms and are not redistributed in this repository.

The imported Python file's SHA-256 was
`7d6397fa912d2a52027b441675702a8356a6a26f80c642a41ed547285531096a`.
The intermediate structure migration moved it to `vendor/theme-ga/theme-ga.py`
without source changes; this port removes that unused Python implementation.
