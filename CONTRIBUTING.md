# Contributing

## Issues And Beads

Use GitHub issues for external intake that is safe to share in the repository.
Use Beads (`bd`) for durable implementation tracking inside the repo when you
are doing local development. Do not edit `.beads/issues.jsonl` directly.

Never place vulnerability details, secrets, customer data, private project
paths, or private incident notes in public GitHub issues.

## Pull Requests

1. Start from an issue or Beads item when possible.
2. Create a branch from `main`.
3. Use signed commits.
4. Keep history linear and changes focused.
5. Run the relevant tests, linters, and builds, or document why the change is
   docs-only.
6. Include the verification performed in the pull request body.
7. Wait for required checks and Open Source Maintainers review.

## Verification

Run the local gates before opening a pull request:

```bash
npm run lint
npm run typecheck
npm run build
cd server && cargo clippy --release && cargo test --release
```

For release, workflow, filesystem, or security-sensitive changes, also run:

```bash
gitleaks detect --no-git --redact --source .
```

## Release Hygiene

Sweetgrass is a fork and continuation of an upstream MIT project. Keep upstream
attribution in [README.md](README.md), [NOTICE](NOTICE), and [LICENSE](LICENSE)
when changing public packaging or project identity.
