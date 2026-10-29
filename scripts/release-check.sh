#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
printf '\nAll local compilation/test gates passed. Manual GUI acceptance checklist still required.\n'
