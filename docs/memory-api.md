# Memory Capture API (stable)

Sweetgrass exposes the project knowledge base
(`.beads/memory/knowledge.jsonl`) over a small HTTP API served by the Rust
backend (default `http://localhost:3008`; override the port with `PORT`).

`POST /api/memory` is the **stable capture endpoint**: it is how the
Sweetgrass UI and orchestration/coding agents append `learned` /
`investigation` entries during work. Capture is agent-driven — agents call
this endpoint on instruction (see [`docs/agents-snippet.md`](agents-snippet.md));
nothing is recorded autonomously.

## Stability policy

- The request/response shapes below are a stable contract: changes are
  additive only (new optional fields). Renames, removals, or semantic changes
  require a new versioned path.
- Unknown request fields are ignored; clients must tolerate new response
  fields.

## POST /api/memory — create an entry

Appends one well-formed JSONL line to `<path>/.beads/memory/knowledge.jsonl`,
creating the file (and `.beads/memory/`) if needed.

### Request

`Content-Type: application/json`

| Field     | Type     | Required | Default         | Notes |
|-----------|----------|----------|-----------------|-------|
| `path`    | string   | yes      | —               | Absolute path to the project root (the directory containing `.beads/`). Must resolve inside the server user's home directory. |
| `content` | string   | yes      | —               | The insight itself. Keep it self-contained (1–3 sentences). |
| `type`    | string   | no       | `learned`       | Canonical values: `learned`, `investigation`. Other values are accepted for forward compatibility but may not render distinctly in the UI yet. |
| `source`  | string   | no       | `sweetgrass-ui` | Who recorded the entry. Agents must set this to their own name or role (e.g. `rust-supervisor`, `orchestrator`). |
| `key`     | string   | no       | `mem-{ts}`      | Unique entry id. When omitted, the server generates `mem-{unix-ts}` and de-duplicates with a numeric suffix (`mem-{ts}-2`, …). Supplying a key that already exists is rejected with `409`. |
| `tags`    | string[] | no       | `[]`            | Free-form lowercase tags used for filtering. |
| `bead`    | string   | no       | `""`            | Bead id the entry relates to (links the entry to the issue in the UI). |

`ts` is always set server-side to the current unix time in seconds.

### Responses

- `200 OK` — `{ "success": true, "entry": { key, type, content, source, tags, ts, bead } }`.
  The returned `entry` is exactly the JSONL line that was appended.
- `400 Bad Request` — `{ "error": "path is required" }` or
  `{ "error": "content is required" }`.
- `403 Forbidden` — `{ "error": ... }` when `path` fails validation
  (outside the home directory, or path traversal).
- `409 Conflict` — `{ "error": "Entry with key '…' already exists" }` when an
  explicitly supplied `key` is taken. Retry without `key` or pick a new one.
- `500 Internal Server Error` — `{ "error": ... }` on I/O failures.

### Example

```bash
curl -s -X POST "${SWEETGRASS_URL:-http://localhost:3008}/api/memory" \
  -H 'Content-Type: application/json' \
  -d "{
    \"path\": \"$(git rev-parse --show-toplevel)\",
    \"type\": \"investigation\",
    \"content\": \"Root cause of the stale board: the watcher only observes issues.jsonl, not the Dolt refs.\",
    \"source\": \"detective\",
    \"tags\": [\"memory\", \"watcher\"],
    \"bead\": \"sweetgrass-abc\"
  }"
```

## Entry schema (JSONL)

Each line of `knowledge.jsonl` is one entry:

```json
{"key":"mem-1769505562","type":"learned","content":"...","source":"rust-supervisor","tags":["rust"],"ts":1769505562,"bead":"sweetgrass-3rj"}
```

## Companion read endpoints

Not part of the capture contract, but useful for recall before implementing:

- `GET /api/memory?path=<project-root>` — all entries (newest first) plus
  aggregate stats.
- `GET /api/memory/stats?path=<project-root>` — counts only.

## Wiring agents up

`AGENTS.md` / `CLAUDE.md` are user-specific (gitignored) in beads
orchestration projects, so the canonical snippet ships as
[`docs/agents-snippet.md`](agents-snippet.md). Install it into a project's
agent instruction files with:

```bash
scripts/install-agents-snippet.sh [target-project-root]
```

This wires orchestration and coding agents to record `learned` /
`investigation` entries (identifying themselves in `source`) while they work.
