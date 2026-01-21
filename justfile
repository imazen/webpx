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

# Run comprehensive profiling benchmarks
bench-profile:
    cargo bench --all-features --bench profile

# Run allocation profiler (generates dhat-heap.json)
alloc-profile:
    cargo run --release --all-features --example alloc_profile
    @echo "View allocation data at: https://nnethercote.github.io/dh_view/dh_view.html"

# Run quick encoder benchmarks only
bench-encode:
    cargo bench --all-features --bench profile -- encode/

# Run quick decoder benchmarks only
bench-decode:
    cargo bench --all-features --bench profile -- decode/

# Run scaling benchmarks (shows size vs time relationship)
bench-scaling:
    cargo bench --all-features --bench profile -- scaling/

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
