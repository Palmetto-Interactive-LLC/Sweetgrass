GITLEAKS_VERSION ?= 8.30.1
GITLEAKS ?= $(or $(shell command -v gitleaks),.cache/gitleaks/gitleaks)
.PHONY: ci lint typecheck build test audit secrets

ci: lint typecheck build test audit secrets

lint:
	npm run lint

typecheck:
	npm run typecheck

# The static export has to finish first: rust-embed compiles ../out into the server binary.
build:
	npm run build

test:
	cd server && cargo clippy --locked --release && cargo test --locked --release

audit:
	npm audit --audit-level=high
	cd server && cargo audit

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
