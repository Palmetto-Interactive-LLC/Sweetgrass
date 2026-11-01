# Maintenance and CI

Sweetgrass remains a Palmetto-maintained open-source project. Use Node 24 and Rust 1.95.0. Run npm ci --ignore-scripts, install cargo-audit 0.22.2, and run make ci. The release build requires the frontend static export before compiling the Rust server. Cargo commands use the committed lockfile.

Depot CI runs lint, TypeScript, the static build, Clippy, Rust tests, npm audit at high severity, cargo audit, and a full-history secret scan. The Linux release jobs use Depot GitHub Actions runners; macOS and Windows jobs retain native GitHub runners. This preserves a single artifact set for the npm installer. No release was published as part of this maintenance update.

Dependabot opens weekly npm, Cargo and GitHub Actions updates. Review and merge only after CI passes. Check full-SHA action pins in .depot/workflows manually each month, because the GitHub Actions updater does not manage this custom workflow directory. Avoid unreviewed major-version force fixes.

## Dependency review, 2026-11-01

Compatible npm updates resolved the two high-severity findings in the prior lockfile. Moderate advisories remain in transitive frontend/development dependencies; the audit output remains visible in CI. ESLint is on 9.39.5 to retain compatibility with eslint-plugin-import; version 10 failed its runtime API integration. eslint-config-next is aligned with Next 16 and uses flat configuration. The new set-state-in-effect rule reports existing effects as warnings pending a behavior-preserving refactor; existing error gates remain enabled.

Cargo audit reports no known vulnerabilities. It reports RUSTSEC-2024-0449: unsafe-libyaml, pulled in by deprecated serde_yaml, is unmaintained. Replacing the YAML parser requires compatibility tests for agent frontmatter edits and beads configuration; do not simply substitute a fork using the same unmaintained backend. The warning is not suppressed.
