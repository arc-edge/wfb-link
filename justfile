fmt:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

check:
    cargo check --workspace

verify: fmt clippy test
