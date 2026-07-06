# Security Policy

Sweetgrass is a local developer tool that reads Beads project data, local git
state, optional agent definitions, and GitHub pull-request metadata. Treat
changes to filesystem access, command execution, repository discovery, GitHub
authentication, release packaging, and workflow permissions as
security-sensitive.

## Reporting A Vulnerability

Please do not open a public GitHub issue for suspected vulnerabilities.

Use GitHub private vulnerability reporting for this repository. If that is not
available, contact the maintainers privately. Include affected versions,
reproduction steps, impact, and whether local files, git credentials, GitHub
tokens, or project memory data may be exposed.

## Supported Versions

The default branch and the latest GitHub Release are supported.

| Version | Supported |
| --- | --- |
| `main` | Yes |
| latest release | Yes |

## Baseline Expectations

- Do not commit populated `.env` files, private keys, tokens, API keys, or
  generated credentials.
- Keep Beads project data, `.claude/` agent definitions, local screenshots,
  editor history, and handoff notes out of the public repository unless they
  are intentionally sanitized examples.
- Keep workflows least-privileged and pin third-party GitHub Actions by commit
  SHA.
- Do not log secrets, full environment dumps, GitHub tokens, or private project
  paths.
- Treat npm postinstall, release assets, and downloaded binaries as supply-chain
  sensitive.
- Validate filesystem paths before reading or writing outside a selected Beads
  project.

## Coordinating Fixes

Security work may be tracked privately in GitHub Security Advisories. Public
issues are appropriate for non-sensitive bugs and feature requests, but do not
copy exploit details, secrets, customer data, or private advisory content into
public issues.
