.PHONY: build check test clean fmt clippy run-server run-server-sqlite run-server-tls run-agent run-agent-tls node-list status integration-test test-all gen-certs

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

run-server-tls:
	cargo run -p pacinet-server -- --port 50054 --db pacinet.db \
		--ca-cert certs/ca.pem --tls-cert certs/server.pem --tls-key certs/server-key.pem \
		--metrics-port 9090

run-agent:
	cargo run -p pacinet-agent -- --controller http://127.0.0.1:50054

run-agent-tls:
	cargo run -p pacinet-agent -- --controller https://127.0.0.1:50054 \
		--ca-cert certs/ca.pem --tls-cert certs/agent.pem --tls-key certs/agent-key.pem

gen-certs:
	bash scripts/gen-certs.sh

node-list:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 node list

status:
	cargo run -p pacinet-cli -- --server http://127.0.0.1:50054 status

integration-test:
	cargo test --test integration -p pacinet-server

test-all:
	cargo test --workspace
	cargo clippy --workspace -- -D warnings
