.PHONY: build check test clean fmt clippy run-server run-agent

build:
	cargo build

check:
	cargo check

test:
	cargo test

clean:
	cargo clean

fmt:
	cargo fmt

clippy:
	cargo clippy -- -D warnings

run-server:
	cargo run -p pacinet-server -- --port 50054

run-agent:
	cargo run -p pacinet-agent -- --controller http://127.0.0.1:50054

node-list:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 node list

status:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 status
