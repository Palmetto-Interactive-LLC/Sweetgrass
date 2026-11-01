# `make ci` is the one command CI runs (Depot CI and a laptop alike).
# The static export has to finish before cargo: rust-embed compiles ../out into the binary.
GITLEAKS_VERSION ?= 8.30.1
GITLEAKS ?= $(or $(shell command -v gitleaks),.cache/gitleaks/gitleaks)

.PHONY: ci check build server secrets

ci: check build server secrets

check:
	npm ci
	npm run lint
	npm run typecheck

build:
	npm run build

server:
	cd server && cargo clippy --release && cargo test --release

# Scans the full git history. Downloads a pinned, checksum-verified gitleaks when none is installed.
secrets:
	@if [ ! -x "$(GITLEAKS)" ]; then \
	  set -e; arch=$$(uname -m | sed 's/aarch64/arm64/;s/x86_64/x64/'); os=$$(uname -s | tr A-Z a-z); \
	  art=gitleaks_$(GITLEAKS_VERSION)_$${os}_$${arch}.tar.gz; \
	  base=https://github.com/gitleaks/gitleaks/releases/download/v$(GITLEAKS_VERSION); \
	  mkdir -p .cache/gitleaks; \
	  curl -fsSLo .cache/gitleaks/$$art $$base/$$art; \
	  curl -fsSLo .cache/gitleaks/sums.txt $$base/gitleaks_$(GITLEAKS_VERSION)_checksums.txt; \
	  (cd .cache/gitleaks && grep "  $$art$$" sums.txt | sha256sum -c -); \
	  tar -xzf .cache/gitleaks/$$art -C .cache/gitleaks gitleaks; \
	fi
	$(GITLEAKS) detect --source . --redact --no-banner
