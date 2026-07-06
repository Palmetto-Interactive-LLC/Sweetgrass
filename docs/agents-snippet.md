# AGENTS.md / CLAUDE.md snippet — Sweetgrass knowledge capture

`AGENTS.md` and `CLAUDE.md` are user-specific (gitignored) in beads
orchestration projects, so this snippet ships here as a tracked template.
Install it into a project's agent instruction files with:

```bash
scripts/install-agents-snippet.sh [target-project-root]
```

The installer is idempotent: it replaces an existing block (matched by the
markers below) or appends one, creating `AGENTS.md`/`CLAUDE.md` if missing.
Everything between the BEGIN/END markers below is the paste-ready snippet.

<!-- BEGIN SWEETGRASS KNOWLEDGE CAPTURE v:1 -->
## Knowledge Capture (Sweetgrass memory)

This project keeps a knowledge base at `.beads/memory/knowledge.jsonl`,
browsable in the Sweetgrass Memory panel. Capture is agent-driven: nothing
records automatically — record entries yourself as part of doing work.

When you learn something reusable (a gotcha, convention, root cause, or
pattern), record it before finishing the task via the stable capture API:

```bash
curl -s -X POST "${SWEETGRASS_URL:-http://localhost:3008}/api/memory" \
  -H 'Content-Type: application/json' \
  -d "{
    \"path\": \"$(git rev-parse --show-toplevel)\",
    \"type\": \"learned\",
    \"content\": \"<one self-contained insight, 1-3 sentences>\",
    \"source\": \"<your agent name or role>\",
    \"tags\": [\"<topic>\"],
    \"bead\": \"<BEAD_ID if working one>\"
  }"
```

Rules:

- `type`: `learned` for reusable lessons, conventions, and gotchas;
  `investigation` for root-cause findings from debugging or research.
- `source`: always identify yourself (e.g. `rust-supervisor`,
  `orchestrator`, `claude-code`). Never leave the default.
- `bead`: set to the bead id you are working, when applicable.
- Keep `content` self-contained: what you learned and why it matters,
  understandable without the surrounding conversation.
- Recall before implementing: `GET /api/memory?path=<project-root>` lists
  prior entries (newest first).
- Fallback when no Sweetgrass server is running:
  `bd comment <BEAD_ID> "LEARNED: <insight>"` or
  `bd comment <BEAD_ID> "INVESTIGATION: <finding>"` — the memory-capture
  hook extracts these into the knowledge file.
- Do not edit `.beads/memory/knowledge.jsonl` by hand; go through the API or
  the hook so entries stay well-formed.

Full API contract: `docs/memory-api.md` in the Sweetgrass repo.
<!-- END SWEETGRASS KNOWLEDGE CAPTURE -->
