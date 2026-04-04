.PHONY: all build build-release run watch test test-unit test-integration test-verbose test-property lint fmt fmt-check check audit coverage coverage-text docs bench docker-build docker-run docker-dev release-dry release-tag changelog clean clean-all

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

watch:
	cargo watch -x 'run -- --help'

# ═══════════════════════════════════════════
# Quality
# ═══════════════════════════════════════════
test:
	cargo test --all-features

test-unit:
	cargo test --lib --all-features

test-integration:
	cargo test --test '*' --all-features

test-verbose:
	cargo test --all-features -- --nocapture

test-property:
	PROPTEST_CASES=1000 cargo test --test '*' --features proptest -- --ignored

lint:
	cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic

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
	@echo "Coverage report: coverage/html/index.html"

coverage-text:
	cargo llvm-cov --all-features --workspace

# ═══════════════════════════════════════════
# Documentation
# ═══════════════════════════════════════════
docs:
	cargo doc --all-features --no-deps --document-private-items --open

# ═══════════════════════════════════════════
# Benchmarks
# ═══════════════════════════════════════════
bench:
	cargo bench --all-features

# ═══════════════════════════════════════════
# Docker
# ═══════════════════════════════════════════
docker-build:
	docker build -t dreamswarm:latest .

docker-run:
	docker run -it --rm \
		-e ANTHROPIC_API_KEY=$(ANTHROPIC_API_KEY) \
		dreamswarm:latest

docker-dev:
	docker compose up -d
	docker compose exec dreamswarm bash

# ═══════════════════════════════════════════
# Release
# ═══════════════════════════════════════════
release-dry:
	cargo publish --dry-run

release-tag:
	@VERSION=$$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/'); \
	echo "Tagging v$$VERSION"; \
	git tag -a "v$$VERSION" -m "Release v$$VERSION"; \
	git push origin "v$$VERSION"

changelog:
	git-cliff -o CHANGELOG.md

# ═══════════════════════════════════════════
# Cleanup
# ═══════════════════════════════════════════
clean:
	cargo clean
	rm -rf coverage/ dist/

clean-all: clean
	rm -rf ~/.dreamswarm/
