#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --no-default-features --features legacy-ui --all-targets -- -D warnings
cargo test --no-default-features --features legacy-ui --all-targets
cargo clippy --no-default-features --features native-ui --bin quark-native -- -D warnings
cargo test --no-default-features --features native-ui --bin quark-native
python3 -m unittest discover -s scripts -p test_feature_coverage.py
python3 scripts/check-feature-coverage.py
cargo build --release --no-default-features --features native-ui --bin quark-native
printf '\nAll local compilation/test gates passed. Manual GUI acceptance checklist still required.\n'
