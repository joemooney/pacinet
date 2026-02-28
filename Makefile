.PHONY: build check test clean fmt clippy run-server run-server-sqlite run-agent node-list status integration-test test-all

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

run-server-sqlite:
	cargo run -p pacinet-server -- --port 50054 --db pacinet.db

run-agent:
	cargo run -p pacinet-agent -- --controller http://127.0.0.1:50054

node-list:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 node list

status:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 status

integration-test:
	cargo test --test integration -p pacinet-server

test-all:
	cargo test --workspace
	cargo clippy --workspace -- -D warnings
