set windows-shell := ["powershell.exe", "-NoProfile", "-Command"]

build:
    cargo build --workspace

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all -- --check

docs:
    cargo doc --workspace --no-deps

deny:
    cargo deny check

clean:
    cargo clean
