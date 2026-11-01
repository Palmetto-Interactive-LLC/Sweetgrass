.PHONY: ci lint typecheck build test secrets

ci: lint typecheck build test secrets

lint:
	npm run lint

typecheck:
	npm run typecheck

# The static export has to finish first: rust-embed compiles ../out into the server binary.
build:
	npm run build

test:
	cd server && cargo clippy --release && cargo test --release

secrets:
	gitleaks detect --source . --no-banner --redact
