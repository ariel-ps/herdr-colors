#!/bin/sh
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
cargo build --release --locked --manifest-path "$root/Cargo.toml" --target-dir "$root/target"
mkdir -p "$root/bin"
cp "$root/target/release/herdr-colors" "$root/bin/.herdr-colors.$$"
mv "$root/bin/.herdr-colors.$$" "$root/bin/herdr-colors"
"$root/bin/herdr-colors" sync
