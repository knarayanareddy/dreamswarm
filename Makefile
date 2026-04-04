# DreamSwarm Makefile
# Developer convenience targets

.PHONY: help check fmt lint test test-unit test-integration coverage clean run

help:
	@echo "🐝 DreamSwarm Development Commands:"
	@echo "  make check      - Run cargo check"
	@echo "  make fmt        - Format code with rustfmt"
	@echo "  make lint       - Run clippy lints (--pedantic)"
	@echo "  make test       - Run all tests"
	@echo "  make test-unit  - Run unit tests only"
	@echo "  make test-integration - Run integration tests only"
	@echo "  make coverage   - Run tests and generate coverage report"
	@echo "  make clean      - Remove build artifacts"
	@echo "  make run        - Run the application in development mode"

check:
	cargo check

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic

test:
	cargo test

test-unit:
	cargo test --lib

test-integration:
	cargo test --test '*'

coverage:
	cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

clean:
	cargo clean

run:
	cargo run
