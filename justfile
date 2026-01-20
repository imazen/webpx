# webpx development commands

# Run all tests
test:
    cargo test --all-features

# Run clippy
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt --check

# Build with all features
build:
    cargo build --all-features

# Build release
build-release:
    cargo build --release --all-features

# Run benchmarks
bench:
    cargo bench --all-features

# Generate docs
doc:
    cargo doc --all-features --no-deps --open

# Run coverage locally
coverage:
    cargo llvm-cov --all-features --html
    @echo "Coverage report: target/llvm-cov/html/index.html"

# Full CI check (run before committing)
ci: fmt-check clippy test
    @echo "All CI checks passed!"

# Clean build artifacts
clean:
    cargo clean
