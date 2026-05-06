.PHONY: build run release check clean install

build:
	cargo build --manifest-path app/Cargo.toml

run: build
	cargo run --manifest-path app/Cargo.toml

release:
	cargo build --release --manifest-path app/Cargo.toml

check:
	cargo clippy --manifest-path app/Cargo.toml -- -D warnings

clean:
	cargo clean --manifest-path app/Cargo.toml

install: release
	cargo install --path app
