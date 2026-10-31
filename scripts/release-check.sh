#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
python3 -m unittest discover -s scripts -p test_feature_coverage.py
python3 scripts/check-feature-coverage.py
cargo build --release
printf '\nAll local compilation/test gates passed. Manual GUI acceptance checklist still required.\n'
