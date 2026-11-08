.PHONY: web build test image linux linux-amd64 linux-arm64

web:
	cd web && npm install && npm run build

# Native Linux musl release. darwin and windows are synced from this later.
build: web
	./scripts/release-musl.sh

linux: build

linux-amd64: web
	./scripts/release-musl.sh amd64

linux-arm64: web
	./scripts/release-musl.sh arm64

test:
	cargo test

image:
	docker build -t loopback-mcp-proxy:local .
