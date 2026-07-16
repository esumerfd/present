.PHONY: build run demo release check clean install

CARGO_TARGET_DIR := gen/build

build:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo build --manifest-path app/Cargo.toml

run: build
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo run --manifest-path app/Cargo.toml

demo: build
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo run --manifest-path app/Cargo.toml -- assets/demo

release:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo build --release --manifest-path app/Cargo.toml

check:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo clippy --manifest-path app/Cargo.toml -- -D warnings

clean:
	CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) cargo clean --manifest-path app/Cargo.toml

install: release
	cargo install --path app --target-dir $(CARGO_TARGET_DIR)
