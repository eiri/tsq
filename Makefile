.DEFAULT = run

.PHONY: all
all: run

.PHONY: run
run:
	cargo run

.PHONY: check
check:
	cargo fmt --check
	cargo test
	$(MAKE) lint

.PHONY: build
build:
	cargo build

.PHONY: release
release:
	cargo build --release

.PHONY: test
test:
	cargo test

.PHONY: lint
lint:
	cargo clippy --no-deps --all-targets --all-features -- -D warnings

.PHONY: clean
clean:
	cargo clean
