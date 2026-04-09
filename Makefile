.PHONY: all build test lint fmt check clean release docker docs bench

# ═══════════════════════════════════════════
# Development
# ═══════════════════════════════════════════

all: fmt lint test build

build:
	cargo build

build-release:
	cargo build --release

run:
	cargo run

# ═══════════════════════════════════════════
# Quality
# ═══════════════════════════════════════════

test:
	cargo test --all-features

test-unit:
	cargo test --lib --all-features

test-integration:
	cargo test --test '*' --all-features

lint:
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets --all-features

# ═══════════════════════════════════════════
# Security
# ═══════════════════════════════════════════

audit:
	cargo deny check
	cargo audit

# ═══════════════════════════════════════════
# Coverage
# ═══════════════════════════════════════════

coverage:
	cargo llvm-cov --all-features --workspace --html --output-dir coverage
	@echo "📊 Coverage report: coverage/html/index.html"

# ═══════════════════════════════════════════
# Documentation
# ═══════════════════════════════════════════

docs:
	cargo doc --all-features --no-deps --document-private-items --open
