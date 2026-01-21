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

# Run memory profiler with heaptrack (captures ALL allocations including C code)
mem-profile:
    cargo build --release --all-features --example alloc_profile
    heaptrack ./target/release/examples/alloc_profile
    @echo ""
    @echo "Analyze with: heaptrack_print heaptrack.alloc_profile.*.zst"
    @echo "Or GUI: heaptrack_gui heaptrack.alloc_profile.*.zst"

# Print heaptrack results (peak memory consumers)
mem-profile-print:
    heaptrack_print heaptrack.alloc_profile.*.zst | head -150

# Quick memory profiler (without heaptrack, just RSS tracking)
mem-profile-quick:
    cargo run --release --all-features --example alloc_profile

# Collect memory formula data (run specific config with heaptrack)
mem-formula size="512" mode="lossy" quality="85" method="4":
    cargo build --release --all-features --example mem_formula
    heaptrack ./target/release/examples/mem_formula --size {{size}} --mode {{mode}} --quality {{quality}} --method {{method}}

# Run batch memory formula collection
mem-formula-batch:
    cargo build --release --all-features --example mem_formula
    ./target/release/examples/mem_formula --batch

# Generate memory formula sweep configs
mem-formula-sweep:
    cargo build --release --all-features --example mem_formula
    ./target/release/examples/mem_formula --sweep

# Comprehensive heaptrack data collection for formula derivation
mem-formula-collect:
    #!/usr/bin/env bash
    set -e
    cargo build --release --all-features --example mem_formula
    mkdir -p mem_data
    for size in 128 256 512 1024 2048; do
        for mode in lossy lossless; do
            for method in 0 4 6; do
                echo "=== size=$size mode=$mode method=$method ==="
                heaptrack ./target/release/examples/mem_formula \
                    --size $size --mode $mode --method $method 2>&1 | \
                    tee -a mem_data/results_${mode}.txt | grep -E "(peak heap|Config:)"
            done
        done
    done
    echo "Results saved to mem_data/"

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
