set shell := ["bash", "-euo", "pipefail", "-c"]

check:
    cargo fmt -- --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked

provider-check:
    cargo test --locked --test provider --test isolation

catalog-check:
    cargo clippy --locked --all-targets --features github-catalog -- -D warnings
    cargo test --locked --features github-catalog

github-negative-check:
    cargo test --locked --features github-catalog --test github_provider -- --ignored
