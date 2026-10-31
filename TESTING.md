# Quark feature testing

Every current user-facing behaviour is recorded in [tests/feature-coverage.json](tests/feature-coverage.json). The register contains **185 features**. Each applicable feature must reference at least **two distinct, meaningful, runnable tests**. Tests may cover several connected behaviours when they actually assert those outcomes. Duplicate references and ignored tests do not satisfy the minimum.

The release check runs formatting, clippy with warnings denied, Rust tests, the coverage-gate regressions, the feature-register validator and an optimized build:

```sh
./scripts/release-check.sh
```

The checks can also run separately (use `python` where Python 3 is installed under that name):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
python3 -m unittest discover -s scripts -p test_feature_coverage.py
python3 scripts/check-feature-coverage.py
cargo build --release
```

The validator asks Cargo for its actual test inventory, including a separate inventory of ignored tests. It rejects missing names, fewer than two distinct runnable tests, invalid metadata and duplicate feature IDs. Eight Python regressions exercise the validator itself. CI runs it on Linux, macOS and Windows. Unix-specific real-PTY/process-group features are explicitly marked in the register; they do not claim ConPTY coverage.

The register is a reviewed feature inventory, **not a line-coverage percentage** or an automatic proof that every assertion is useful. Adding a feature requires adding its register entry and two behavioural cases, normally a successful interaction and a boundary/failure/regression case. Removing or renaming a referenced test fails the gate. Reviewers must keep the inventory complete and assess whether both tests exercise the promised behaviour.

The headless egui harness sends pointer, keyboard and clipboard events through the real application update loop. It owns a temporary configuration directory for every test; tests do not replace the user's settings, recovery files or installed extensions. Local filesystem and PTY tests exercise genuine files/processes, with bounded waits and cleanup. File-chooser handlers have explicit path seams so Save As, theme import/export and persistence failures can be exercised without opening an OS dialog during CI.

Native host integration still needs the [manual acceptance checklist](scripts/manual-acceptance.md): window-manager effects, dialogs and cancellation, file-manager launching, DPI, clipboard/IME, Windows ConPTY, macOS shortcuts, Wayland, actual Hydrogen compiler versions and authenticated Git remotes. Automated Git-button tests validate argument routing/busy guards in disposable folders; they do not contact authenticated remotes. Catalogue Add/discovery and auto-update transports are placeholders until repositories are configured, and tests assert that those controls do not start a network job.
