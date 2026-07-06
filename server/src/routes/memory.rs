//! Memory API route handlers.
//!
//! Provides endpoints for reading, editing, and deleting knowledge base entries
//! from `.beads/memory/knowledge.jsonl` files.

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::validate_path_security;

// ---------------------------------------------------------------------------
// Entry types
// ---------------------------------------------------------------------------

/// The two canonical entry types that every knowledge base starts with.
pub const CANONICAL_ENTRY_TYPES: [&str; 2] = ["learned", "investigation"];

/// Additional first-class entry types recognized by the UI. The overall set
/// is open-ended: any well-formed type string is accepted so the knowledge
/// base can grow richer without schema breaks.
// Referenced in tests and mirrored by KNOWN_MEMORY_TYPES in src/types/index.ts.
#[allow(dead_code)]
pub const EXTENDED_ENTRY_TYPES: [&str; 3] = ["decision", "gotcha", "convention"];

/// Default entry type applied when none is supplied.
pub const DEFAULT_ENTRY_TYPE: &str = "learned";

/// Maximum length of an entry type string.
const MAX_ENTRY_TYPE_LEN: usize = 32;

/// Normalize and validate an entry type.
///
/// - `None` or blank input falls back to [`DEFAULT_ENTRY_TYPE`].
/// - Input is trimmed and lowercased (types are case-insensitive).
/// - Any type is allowed (extensible), as long as it is composed of
///   `[a-z0-9_-]` and at most [`MAX_ENTRY_TYPE_LEN`] characters.
fn normalize_entry_type(raw: Option<&str>) -> Result<String, String> {
    let trimmed = raw.map(str::trim).unwrap_or("");
    if trimmed.is_empty() {
        return Ok(DEFAULT_ENTRY_TYPE.to_string());
    }

    let normalized = trimmed.to_lowercase();
    if normalized.len() > MAX_ENTRY_TYPE_LEN {
        return Err(format!(
            "type must be at most {} characters",
            MAX_ENTRY_TYPE_LEN
        ));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(
            "type may only contain letters, digits, hyphens, and underscores".to_string(),
        );
    }

    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single memory/knowledge entry from the JSONL file.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryEntry {
    pub key: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub content: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub ts: i64,
    #[serde(default)]
    pub bead: String,
}

/// Aggregated statistics about memory entries.
///
/// `learned` and `investigation` remain dedicated fields for backward
/// compatibility; `by_type` carries counts for every entry type present,
/// including non-canonical ones (e.g. decision, gotcha, convention).
#[derive(Debug, Serialize)]
pub struct MemoryStats {
    pub total: usize,
    pub learned: usize,
    pub investigation: usize,
    pub archived: usize,
    pub by_type: BTreeMap<String, usize>,
}

/// Facet value counts computed over the full (unfiltered) entry set, so the
/// UI can render filter options with counts even while a filter is active.
#[derive(Debug, Serialize)]
pub struct MemoryFacets {
    pub types: BTreeMap<String, usize>,
    pub sources: BTreeMap<String, usize>,
    pub tags: BTreeMap<String, usize>,
}

/// Response for the list memory endpoint.
#[derive(Debug, Serialize)]
pub struct MemoryListResponse {
    pub entries: Vec<MemoryEntry>,
    pub stats: MemoryStats,
    pub facets: MemoryFacets,
}

/// Query parameters for GET endpoints.
///
/// `path` is required. The remaining fields are optional server-side search
/// and facet filters applied by `list_memory` (ignored by `memory_stats`).
#[derive(Debug, Default, Deserialize)]
pub struct MemoryParams {
    pub path: String,
    /// Case-insensitive substring search across content, key, tags, and bead.
    #[serde(default)]
    pub q: Option<String>,
    /// Filter by entry type (e.g. "learned", "investigation").
    #[serde(rename = "type", default)]
    pub entry_type: Option<String>,
    /// Filter by source (case-insensitive exact match).
    #[serde(default)]
    pub source: Option<String>,
    /// Filter to entries carrying this tag (case-insensitive exact match).
    #[serde(default)]
    pub tag: Option<String>,
    /// Inclusive lower bound on `ts`: unix seconds or `YYYY-MM-DD` (start of day, UTC).
    #[serde(default)]
    pub since: Option<String>,
    /// Inclusive upper bound on `ts`: unix seconds or `YYYY-MM-DD` (end of day, UTC).
    #[serde(default)]
    pub until: Option<String>,
}

/// Request body for the update memory endpoint.
#[derive(Debug, Deserialize)]
pub struct UpdateMemoryRequest {
    pub path: String,
    pub key: String,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Request body for the create memory endpoint.
#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(rename = "type", default)]
    pub entry_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub bead: String,
}

/// Request body for the delete memory endpoint.
#[derive(Debug, Deserialize)]
pub struct DeleteMemoryRequest {
    pub path: String,
    pub key: String,
    #[serde(default)]
    pub archive: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the path to the active knowledge file.
fn knowledge_path(project_path: &Path) -> PathBuf {
    project_path
        .join(".beads")
        .join("memory")
        .join("knowledge.jsonl")
}

/// Build the path to the archive knowledge file.
fn archive_path(project_path: &Path) -> PathBuf {
    project_path
        .join(".beads")
        .join("memory")
        .join("knowledge.archive.jsonl")
}

/// Parse a JSONL file into a list of `MemoryEntry` values.
///
/// Missing files are treated as empty. Malformed lines are skipped with a
/// warning logged via `tracing`.
fn read_entries(path: &PathBuf) -> Result<Vec<MemoryEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let mut entries = Vec::new();
    for (line_num, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<MemoryEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                tracing::warn!(
                    "Failed to parse memory entry at line {}: {} - {}",
                    line_num + 1,
                    e,
                    line
                );
            }
        }
    }

    Ok(entries)
}

/// Write a list of entries back to a JSONL file (overwrite).
fn write_entries(path: &PathBuf, entries: &[MemoryEntry]) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to open file for writing: {}", e))?;

    let mut writer = std::io::BufWriter::new(file);
    for entry in entries {
        let json_line = serde_json::to_string(entry)
            .map_err(|e| format!("Failed to serialize entry: {}", e))?;
        writeln!(writer, "{}", json_line)
            .map_err(|e| format!("Failed to write to file: {}", e))?;
    }
    writer
        .flush()
        .map_err(|e| format!("Failed to flush file: {}", e))?;

    Ok(())
}

/// Append a single entry to a JSONL file (creating the file if needed).
fn append_entry(path: &PathBuf, entry: &MemoryEntry) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open archive file: {}", e))?;

    let mut writer = std::io::BufWriter::new(file);
    let json_line = serde_json::to_string(entry)
        .map_err(|e| format!("Failed to serialize entry: {}", e))?;
    writeln!(writer, "{}", json_line)
        .map_err(|e| format!("Failed to write to archive: {}", e))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush archive: {}", e))?;

    Ok(())
}

/// Count the number of entries in a JSONL file (for archive stats).
fn count_entries(path: &PathBuf) -> usize {
    if !path.exists() {
        return 0;
    }

    match std::fs::read_to_string(path) {
        Ok(contents) => contents
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty()
            })
            .count(),
        Err(_) => 0,
    }
}

/// Compute stats from a list of entries plus an archived count.
fn compute_stats(entries: &[MemoryEntry], archived: usize) -> MemoryStats {
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        *by_type.entry(entry.entry_type.clone()).or_insert(0) += 1;
    }

    let learned = by_type.get(CANONICAL_ENTRY_TYPES[0]).copied().unwrap_or(0);
    let investigation = by_type
        .get(CANONICAL_ENTRY_TYPES[1])
        .copied()
        .unwrap_or(0);

    MemoryStats {
        total: entries.len(),
        learned,
        investigation,
        archived,
        by_type,
    }
}

/// Compute facet value counts (type, source, tag) from a list of entries.
fn compute_facets(entries: &[MemoryEntry]) -> MemoryFacets {
    let mut types: BTreeMap<String, usize> = BTreeMap::new();
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();

    for entry in entries {
        if !entry.entry_type.is_empty() {
            *types.entry(entry.entry_type.clone()).or_insert(0) += 1;
        }
        if !entry.source.is_empty() {
            *sources.entry(entry.source.clone()).or_insert(0) += 1;
        }
        for tag in &entry.tags {
            if !tag.is_empty() {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }
    }

    MemoryFacets { types, sources, tags }
}

/// Parse a date-range bound: either unix epoch seconds or a `YYYY-MM-DD`
/// calendar date interpreted in UTC. For `until` bounds, a calendar date
/// resolves to the end of that day so the range is inclusive.
fn parse_ts_bound(value: &str, end_of_day: bool) -> Result<i64, String> {
    let trimmed = value.trim();
    if let Ok(ts) = trimmed.parse::<i64>() {
        return Ok(ts);
    }

    let date = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|_| {
        format!(
            "Invalid date '{}': expected unix seconds or YYYY-MM-DD",
            trimmed
        )
    })?;

    let (h, m, s) = if end_of_day { (23, 59, 59) } else { (0, 0, 0) };
    let dt = date
        .and_hms_opt(h, m, s)
        .ok_or_else(|| format!("Invalid date '{}'", trimmed))?;

    Ok(dt.and_utc().timestamp())
}

/// Server-side search + facet filter parsed from `MemoryParams`.
#[derive(Debug, Default)]
struct MemoryFilter {
    /// Lowercased substring query.
    q: Option<String>,
    entry_type: Option<String>,
    source: Option<String>,
    tag: Option<String>,
    since: Option<i64>,
    until: Option<i64>,
}

impl MemoryFilter {
    /// Build a filter from query params, validating date bounds.
    /// Blank/whitespace-only params are treated as absent.
    fn try_from_params(params: &MemoryParams) -> Result<Self, String> {
        fn non_blank(value: &Option<String>) -> Option<String> {
            value
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        }

        let since = non_blank(&params.since)
            .map(|s| parse_ts_bound(&s, false))
            .transpose()?;
        let until = non_blank(&params.until)
            .map(|s| parse_ts_bound(&s, true))
            .transpose()?;

        Ok(Self {
            q: non_blank(&params.q).map(|s| s.to_lowercase()),
            entry_type: non_blank(&params.entry_type),
            source: non_blank(&params.source),
            tag: non_blank(&params.tag),
            since,
            until,
        })
    }

    /// Whether an entry passes every active filter.
    fn matches(&self, entry: &MemoryEntry) -> bool {
        if let Some(entry_type) = &self.entry_type {
            if !entry.entry_type.eq_ignore_ascii_case(entry_type) {
                return false;
            }
        }
        if let Some(source) = &self.source {
            if !entry.source.eq_ignore_ascii_case(source) {
                return false;
            }
        }
        if let Some(tag) = &self.tag {
            if !entry.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                return false;
            }
        }
        if let Some(since) = self.since {
            if entry.ts < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if entry.ts > until {
                return false;
            }
        }
        if let Some(q) = &self.q {
            // Mirrors the client-side fallback: content, key, tags, bead.
            let matched = entry.content.to_lowercase().contains(q)
                || entry.key.to_lowercase().contains(q)
                || entry.tags.iter().any(|t| t.to_lowercase().contains(q))
                || entry.bead.to_lowercase().contains(q);
            if !matched {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/memory?path={project_path}
///
/// Reads all entries from the active knowledge file and returns them along
/// with aggregate statistics and facet counts. Entries are sorted by `ts`
/// descending (newest first).
///
/// Optional server-side filters:
/// - `q` — case-insensitive substring search over content, key, tags, bead
/// - `type` — entry type (e.g. `learned`, `investigation`)
/// - `source` — source (case-insensitive exact match)
/// - `tag` — entries carrying the tag (case-insensitive exact match)
/// - `since` / `until` — inclusive `ts` range; unix seconds or `YYYY-MM-DD`
///
/// `stats` and `facets` are always computed over the full (unfiltered) set so
/// the UI keeps stable totals and filter options while a filter is active.
pub async fn list_memory(Query(params): Query<MemoryParams>) -> impl IntoResponse {
    let project_path = PathBuf::from(&params.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e })),
        );
    }

    let filter = match MemoryFilter::try_from_params(&params) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    let kpath = knowledge_path(&project_path);
    let apath = archive_path(&project_path);

    let entries = match read_entries(&kpath) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    let archived = count_entries(&apath);
    let stats = compute_stats(&entries, archived);
    let facets = compute_facets(&entries);

    let mut entries: Vec<MemoryEntry> =
        entries.into_iter().filter(|e| filter.matches(e)).collect();

    // Sort by ts descending (newest first)
    entries.sort_by_key(|e| std::cmp::Reverse(e.ts));

    (
        StatusCode::OK,
        Json(serde_json::json!(MemoryListResponse {
            entries,
            stats,
            facets
        })),
    )
}

/// GET /api/memory/stats?path={project_path}
///
/// Lightweight endpoint returning only aggregate statistics (no entry content).
pub async fn memory_stats(Query(params): Query<MemoryParams>) -> impl IntoResponse {
    let project_path = PathBuf::from(&params.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e })),
        );
    }

    let kpath = knowledge_path(&project_path);
    let apath = archive_path(&project_path);

    let entries = match read_entries(&kpath) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    let archived = count_entries(&apath);
    let stats = compute_stats(&entries, archived);

    (StatusCode::OK, Json(serde_json::json!(stats)))
}

/// POST /api/memory — create and append a new knowledge entry.
///
/// This is the stable capture API used by the Sweetgrass UI and by
/// orchestration/coding agents recording knowledge entries during work
/// (agents identify themselves in `source`). The contract is documented in
/// `docs/memory-api.md`; agent wiring lives in `docs/agents-snippet.md`
/// (installed via `scripts/install-agents-snippet.sh`). Changes to this
/// endpoint must remain backward-compatible (additive only).
///
/// Behavior:
/// - Validates the project path and requires non-empty `content`.
/// - Normalizes the entry `type`: any well-formed type is accepted (not just
///   `learned`/`investigation`), defaulting to `learned` when omitted.
/// - Generates a `mem-{ts}` key when none is supplied, de-duplicating with a
///   numeric suffix when that key is already taken.
/// - Returns `409 Conflict` when an explicitly supplied key already exists.
/// - Defaults `source` to `sweetgrass-ui`, `ts` to now, then appends a
///   well-formed line to `knowledge.jsonl`.
pub async fn create_memory(Json(payload): Json<CreateMemoryRequest>) -> impl IntoResponse {
    if payload.path.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "path is required" })),
        );
    }
    if payload.content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "content is required" })),
        );
    }

    let project_path = PathBuf::from(&payload.path);
    if let Err(e) = validate_path_security(&project_path) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": e })));
    }

    let entry_type = match normalize_entry_type(payload.entry_type.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    let kpath = knowledge_path(&project_path);
    let existing = match read_entries(&kpath) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let key = match payload.key {
        Some(ref k) if !k.trim().is_empty() => {
            let requested = k.trim().to_string();
            if existing.iter().any(|e| e.key == requested) {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": format!("Entry with key '{}' already exists", requested)
                    })),
                );
            }
            requested
        }
        _ => {
            let base = format!("mem-{}", ts);
            let mut candidate = base.clone();
            let mut suffix = 2;
            while existing.iter().any(|e| e.key == candidate) {
                candidate = format!("{}-{}", base, suffix);
                suffix += 1;
            }
            candidate
        }
    };

    let entry = MemoryEntry {
        key,
        entry_type,
        content: payload.content,
        source: payload
            .source
            .filter(|src| !src.trim().is_empty())
            .unwrap_or_else(|| "sweetgrass-ui".to_string()),
        tags: payload.tags,
        ts,
        bead: payload.bead,
    };

    if let Err(e) = append_entry(&kpath, &entry) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true, "entry": entry })),
    )
}

/// PUT /api/memory
///
/// Edit an existing entry by key. Updates `content` and/or `tags` fields.
/// At least one of `content` or `tags` must be provided.
/// The `ts` field is NOT updated (it represents original creation time).
pub async fn update_memory(Json(payload): Json<UpdateMemoryRequest>) -> impl IntoResponse {
    // Validate that at least one field is provided
    if payload.content.is_none() && payload.tags.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "At least one of 'content' or 'tags' must be provided"
            })),
        );
    }

    let project_path = PathBuf::from(&payload.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e })),
        );
    }

    let kpath = knowledge_path(&project_path);

    let mut entries = match read_entries(&kpath) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    // Find the entry with the matching key
    let entry_pos = entries.iter().position(|e| e.key == payload.key);

    let idx = match entry_pos {
        Some(i) => i,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("Entry with key '{}' not found", payload.key)
                })),
            );
        }
    };

    // Update fields
    if let Some(content) = payload.content {
        entries[idx].content = content;
    }
    if let Some(tags) = payload.tags {
        entries[idx].tags = tags;
    }

    // Write back
    if let Err(e) = write_entries(&kpath, &entries) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        );
    }

    let updated_entry = entries[idx].clone();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "entry": updated_entry
        })),
    )
}

/// DELETE /api/memory
///
/// Remove or archive an entry by key.
///
/// - `archive: true`  — Move entry to `knowledge.archive.jsonl`, then remove
///   from `knowledge.jsonl`.
/// - `archive: false` — Permanently delete from `knowledge.jsonl`.
pub async fn delete_memory(Json(payload): Json<DeleteMemoryRequest>) -> impl IntoResponse {
    let project_path = PathBuf::from(&payload.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e })),
        );
    }

    let kpath = knowledge_path(&project_path);

    let mut entries = match read_entries(&kpath) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    };

    // Find the entry with the matching key
    let entry_pos = entries.iter().position(|e| e.key == payload.key);

    let idx = match entry_pos {
        Some(i) => i,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("Entry with key '{}' not found", payload.key)
                })),
            );
        }
    };

    // Remove the entry
    let removed_entry = entries.remove(idx);

    // If archiving, append to archive file
    if payload.archive {
        let apath = archive_path(&project_path);
        if let Err(e) = append_entry(&apath, &removed_entry) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            );
        }
    }

    // Write back the remaining entries
    if let Err(e) = write_entries(&kpath, &entries) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "archived": payload.archive
        })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_entry() {
        let json = r#"{"key":"test-key","type":"learned","content":"Some content","source":"orchestrator","tags":["tag1","tag2"],"ts":1769505562,"bead":"project-id.3"}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.key, "test-key");
        assert_eq!(entry.entry_type, "learned");
        assert_eq!(entry.content, "Some content");
        assert_eq!(entry.source, "orchestrator");
        assert_eq!(entry.tags, vec!["tag1", "tag2"]);
        assert_eq!(entry.ts, 1769505562);
        assert_eq!(entry.bead, "project-id.3");
    }

    #[test]
    fn test_parse_memory_entry_investigation() {
        let json = r#"{"key":"inv-key","type":"investigation","content":"Root cause analysis","source":"detective","tags":["investigation"],"ts":1769505000,"bead":"bd-42.1"}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type, "investigation");
    }

    #[test]
    fn test_parse_memory_entry_defaults() {
        // tags and bead are optional (have defaults)
        let json = r#"{"key":"min-key","type":"learned","content":"Minimal","source":"src","ts":100}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.tags, Vec::<String>::new());
        assert_eq!(entry.bead, "");
    }

    #[test]
    fn test_parse_memory_entry_missing_source() {
        // Legacy entries may predate the source field; they must not be dropped
        let json = r#"{"key":"legacy-key","type":"learned","content":"Old entry","ts":50}"#;
        let entry: MemoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.key, "legacy-key");
        assert_eq!(entry.source, "");
        assert_eq!(entry.ts, 50);
    }

    #[test]
    fn test_serialize_memory_entry() {
        let entry = MemoryEntry {
            key: "k".to_string(),
            entry_type: "learned".to_string(),
            content: "c".to_string(),
            source: "s".to_string(),
            tags: vec!["t".to_string()],
            ts: 42,
            bead: "b".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        // The field should serialize as "type", not "entry_type"
        assert!(json.contains(r#""type":"learned""#));
        assert!(!json.contains("entry_type"));
    }

    #[test]
    fn test_compute_stats() {
        let entries = vec![
            MemoryEntry {
                key: "a".into(),
                entry_type: "learned".into(),
                content: "".into(),
                source: "".into(),
                tags: vec![],
                ts: 1,
                bead: "".into(),
            },
            MemoryEntry {
                key: "b".into(),
                entry_type: "learned".into(),
                content: "".into(),
                source: "".into(),
                tags: vec![],
                ts: 2,
                bead: "".into(),
            },
            MemoryEntry {
                key: "c".into(),
                entry_type: "investigation".into(),
                content: "".into(),
                source: "".into(),
                tags: vec![],
                ts: 3,
                bead: "".into(),
            },
        ];

        let stats = compute_stats(&entries, 5);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.learned, 2);
        assert_eq!(stats.investigation, 1);
        assert_eq!(stats.archived, 5);
        assert_eq!(stats.by_type.get("learned"), Some(&2));
        assert_eq!(stats.by_type.get("investigation"), Some(&1));
    }

    #[test]
    fn test_compute_stats_empty() {
        let stats = compute_stats(&[], 0);
        assert_eq!(stats.total, 0);
        assert_eq!(stats.learned, 0);
        assert_eq!(stats.investigation, 0);
        assert_eq!(stats.archived, 0);
        assert!(stats.by_type.is_empty());
    }

    fn entry_of_type(key: &str, entry_type: &str) -> MemoryEntry {
        MemoryEntry {
            key: key.into(),
            entry_type: entry_type.into(),
            content: "".into(),
            source: "".into(),
            tags: vec![],
            ts: 0,
            bead: "".into(),
        }
    }

    #[test]
    fn test_compute_stats_extended_types() {
        let entries = vec![
            entry_of_type("a", "learned"),
            entry_of_type("b", "decision"),
            entry_of_type("c", "gotcha"),
            entry_of_type("d", "convention"),
            entry_of_type("e", "decision"),
            entry_of_type("f", "runbook"),
        ];

        let stats = compute_stats(&entries, 0);
        assert_eq!(stats.total, 6);
        assert_eq!(stats.learned, 1);
        assert_eq!(stats.investigation, 0);
        assert_eq!(stats.by_type.get("decision"), Some(&2));
        assert_eq!(stats.by_type.get("gotcha"), Some(&1));
        assert_eq!(stats.by_type.get("convention"), Some(&1));
        // Unknown types are still counted — the set is open-ended.
        assert_eq!(stats.by_type.get("runbook"), Some(&1));
    }

    #[test]
    fn test_parse_memory_entry_extended_types() {
        for entry_type in EXTENDED_ENTRY_TYPES {
            let json = format!(
                r#"{{"key":"k","type":"{}","content":"c","source":"s","ts":1}}"#,
                entry_type
            );
            let entry: MemoryEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(entry.entry_type, entry_type);
        }
    }

    #[test]
    fn test_normalize_entry_type_defaults() {
        assert_eq!(normalize_entry_type(None).unwrap(), DEFAULT_ENTRY_TYPE);
        assert_eq!(normalize_entry_type(Some("")).unwrap(), DEFAULT_ENTRY_TYPE);
        assert_eq!(
            normalize_entry_type(Some("   ")).unwrap(),
            DEFAULT_ENTRY_TYPE
        );
    }

    #[test]
    fn test_normalize_entry_type_known_types() {
        for entry_type in CANONICAL_ENTRY_TYPES.iter().chain(&EXTENDED_ENTRY_TYPES) {
            assert_eq!(normalize_entry_type(Some(entry_type)).unwrap(), *entry_type);
        }
    }

    #[test]
    fn test_normalize_entry_type_case_and_whitespace() {
        assert_eq!(normalize_entry_type(Some("Decision")).unwrap(), "decision");
        assert_eq!(normalize_entry_type(Some(" GOTCHA ")).unwrap(), "gotcha");
    }

    #[test]
    fn test_normalize_entry_type_custom_allowed() {
        // Extensible: well-formed non-canonical types are accepted.
        assert_eq!(normalize_entry_type(Some("runbook")).unwrap(), "runbook");
        assert_eq!(
            normalize_entry_type(Some("post_mortem-2")).unwrap(),
            "post_mortem-2"
        );
    }

    #[test]
    fn test_normalize_entry_type_invalid() {
        assert!(normalize_entry_type(Some("has space")).is_err());
        assert!(normalize_entry_type(Some("naïve")).is_err());
        assert!(normalize_entry_type(Some("semi;colon")).is_err());
        let too_long = "x".repeat(MAX_ENTRY_TYPE_LEN + 1);
        assert!(normalize_entry_type(Some(&too_long)).is_err());
    }

    #[test]
    fn test_read_entries_missing_file() {
        let path = PathBuf::from("/nonexistent/path/knowledge.jsonl");
        let result = read_entries(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_count_entries_missing_file() {
        let path = PathBuf::from("/nonexistent/path/knowledge.jsonl");
        assert_eq!(count_entries(&path), 0);
    }

    #[test]
    fn test_write_and_read_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let entries = vec![
            MemoryEntry {
                key: "k1".into(),
                entry_type: "learned".into(),
                content: "content1".into(),
                source: "src".into(),
                tags: vec!["a".into()],
                ts: 100,
                bead: "b1".into(),
            },
            MemoryEntry {
                key: "k2".into(),
                entry_type: "investigation".into(),
                content: "content2".into(),
                source: "src".into(),
                tags: vec![],
                ts: 200,
                bead: "b2".into(),
            },
        ];

        write_entries(&path, &entries).unwrap();
        let read_back = read_entries(&path).unwrap();

        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].key, "k1");
        assert_eq!(read_back[0].entry_type, "learned");
        assert_eq!(read_back[1].key, "k2");
        assert_eq!(read_back[1].entry_type, "investigation");
    }

    #[test]
    fn test_append_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.jsonl");

        let entry1 = MemoryEntry {
            key: "k1".into(),
            entry_type: "learned".into(),
            content: "c1".into(),
            source: "s".into(),
            tags: vec![],
            ts: 1,
            bead: "".into(),
        };

        let entry2 = MemoryEntry {
            key: "k2".into(),
            entry_type: "investigation".into(),
            content: "c2".into(),
            source: "s".into(),
            tags: vec![],
            ts: 2,
            bead: "".into(),
        };

        append_entry(&path, &entry1).unwrap();
        append_entry(&path, &entry2).unwrap();

        let entries = read_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "k1");
        assert_eq!(entries[1].key, "k2");
    }

    #[tokio::test]
    async fn test_create_memory_appends_entry_with_defaults() {
        use axum::response::IntoResponse;

        // validate_path_security requires the project to live inside the
        // user's home directory, so create the temp project there.
        let home = directories::UserDirs::new()
            .expect("home dir")
            .home_dir()
            .to_path_buf();
        let dir = tempfile::Builder::new()
            .prefix(".sweetgrass-test-")
            .tempdir_in(&home)
            .expect("tempdir in home");

        let payload = CreateMemoryRequest {
            path: dir.path().to_string_lossy().into_owned(),
            content: "Manual capture flow works".to_string(),
            key: None,
            entry_type: Some("investigation".to_string()),
            source: None,
            tags: vec!["ui".to_string()],
            bead: "sweetgrass-0e9".to_string(),
        };

        let resp = create_memory(Json(payload)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        let entries = read_entries(&knowledge_path(dir.path())).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.entry_type, "investigation");
        assert_eq!(entry.content, "Manual capture flow works");
        assert_eq!(entry.source, "sweetgrass-ui");
        assert_eq!(entry.tags, vec!["ui"]);
        assert_eq!(entry.bead, "sweetgrass-0e9");
        assert!(entry.key.starts_with("mem-"), "generated key: {}", entry.key);
        assert!(entry.ts > 0);
    }

    #[test]
    fn test_knowledge_path() {
        let project = PathBuf::from("/home/user/project");
        let kp = knowledge_path(&project);
        assert_eq!(
            kp,
            PathBuf::from("/home/user/project/.beads/memory/knowledge.jsonl")
        );
    }

    #[test]
    fn test_archive_path() {
        let project = PathBuf::from("/home/user/project");
        let ap = archive_path(&project);
        assert_eq!(
            ap,
            PathBuf::from("/home/user/project/.beads/memory/knowledge.archive.jsonl")
        );
    }

    // -----------------------------------------------------------------------
    // create_memory handler tests (stable capture API — docs/memory-api.md)
    // -----------------------------------------------------------------------

    /// Temp project dir under `$HOME` so `validate_path_security` accepts it.
    fn home_tempdir() -> tempfile::TempDir {
        let home = directories::UserDirs::new()
            .expect("user dirs")
            .home_dir()
            .to_path_buf();
        tempfile::Builder::new()
            .prefix(".sweetgrass-memory-test-")
            .tempdir_in(home)
            .expect("tempdir under home")
    }

    /// Invoke the handler directly and decode (status, JSON body).
    async fn post_create(req: CreateMemoryRequest) -> (StatusCode, serde_json::Value) {
        let resp = create_memory(Json(req)).await.into_response();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let body = serde_json::from_slice(&bytes).expect("json body");
        (status, body)
    }

    fn create_req(path: &str, content: &str) -> CreateMemoryRequest {
        CreateMemoryRequest {
            path: path.to_string(),
            content: content.to_string(),
            key: None,
            entry_type: None,
            source: None,
            tags: Vec::new(),
            bead: String::new(),
        }
    }

    #[tokio::test]
    async fn test_create_memory_defaults() {
        let dir = home_tempdir();
        let path = dir.path().to_string_lossy().to_string();

        let (status, body) = post_create(create_req(&path, "A reusable insight")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], true);
        assert_eq!(body["entry"]["type"], "learned");
        assert_eq!(body["entry"]["source"], "sweetgrass-ui");
        assert!(body["entry"]["key"].as_str().unwrap().starts_with("mem-"));

        let entries = read_entries(&knowledge_path(dir.path())).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "A reusable insight");
    }

    #[tokio::test]
    async fn test_create_memory_agent_fields_preserved() {
        let dir = home_tempdir();
        let path = dir.path().to_string_lossy().to_string();

        let req = CreateMemoryRequest {
            path,
            content: "Root cause: rust-embed needs ../out before cargo build".to_string(),
            key: Some("inv-rust-embed-out".to_string()),
            entry_type: Some("investigation".to_string()),
            source: Some("rust-supervisor".to_string()),
            tags: vec!["rust".to_string(), "build".to_string()],
            bead: "sweetgrass-3rj".to_string(),
        };
        let (status, body) = post_create(req).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["entry"]["key"], "inv-rust-embed-out");
        assert_eq!(body["entry"]["type"], "investigation");
        assert_eq!(body["entry"]["source"], "rust-supervisor");
        assert_eq!(body["entry"]["bead"], "sweetgrass-3rj");

        let entries = read_entries(&knowledge_path(dir.path())).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tags, vec!["rust", "build"]);
    }

    #[tokio::test]
    async fn test_create_memory_rejects_empty_content() {
        let dir = home_tempdir();
        let path = dir.path().to_string_lossy().to_string();

        let (status, body) = post_create(create_req(&path, "   ")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "content is required");
    }

    #[tokio::test]
    async fn test_create_memory_rejects_empty_path() {
        let (status, body) = post_create(create_req("", "content")).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "path is required");
    }

    #[tokio::test]
    async fn test_create_memory_rejects_path_outside_home() {
        let (status, _body) = post_create(create_req("/etc", "content")).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_create_memory_duplicate_explicit_key_conflicts() {
        let dir = home_tempdir();
        let path = dir.path().to_string_lossy().to_string();

        let mut first = create_req(&path, "first");
        first.key = Some("dup-key".to_string());
        let (status, _) = post_create(first).await;
        assert_eq!(status, StatusCode::OK);

        let mut second = create_req(&path, "second");
        second.key = Some("dup-key".to_string());
        let (status, body) = post_create(second).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("already exists"));

        // The conflicting entry must not have been appended.
        let entries = read_entries(&knowledge_path(dir.path())).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "first");
    }

    #[tokio::test]
    async fn test_create_memory_auto_key_deduplicates() {
        let dir = home_tempdir();
        let path = dir.path().to_string_lossy().to_string();

        // Occupy every auto key the handler could generate in the next few
        // seconds so the generated base key is guaranteed to collide.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let seeded: Vec<MemoryEntry> = (now..now + 30)
            .map(|t| MemoryEntry {
                key: format!("mem-{}", t),
                entry_type: "learned".into(),
                content: "seed".into(),
                source: "test".into(),
                tags: vec![],
                ts: t,
                bead: String::new(),
            })
            .collect();
        let kpath = knowledge_path(dir.path());
        std::fs::create_dir_all(kpath.parent().unwrap()).unwrap();
        write_entries(&kpath, &seeded).unwrap();

        let (status, body) = post_create(create_req(&path, "collides")).await;

        assert_eq!(status, StatusCode::OK);
        let key = body["entry"]["key"].as_str().unwrap().to_string();
        assert!(key.starts_with("mem-"));
        assert!(key.ends_with("-2"), "expected suffixed key, got {}", key);
        let entries = read_entries(&kpath).unwrap();
        assert_eq!(entries.iter().filter(|e| e.key == key).count(), 1);
    }

    // -- Server-side search + facet filters --------------------------------

    fn entry(
        key: &str,
        entry_type: &str,
        content: &str,
        source: &str,
        tags: &[&str],
        ts: i64,
        bead: &str,
    ) -> MemoryEntry {
        MemoryEntry {
            key: key.into(),
            entry_type: entry_type.into(),
            content: content.into(),
            source: source.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ts,
            bead: bead.into(),
        }
    }

    fn params(pairs: &[(&str, &str)]) -> MemoryParams {
        let mut p = MemoryParams {
            path: "/tmp/project".into(),
            ..Default::default()
        };
        for (k, v) in pairs {
            let v = Some(v.to_string());
            match *k {
                "q" => p.q = v,
                "type" => p.entry_type = v,
                "source" => p.source = v,
                "tag" => p.tag = v,
                "since" => p.since = v,
                "until" => p.until = v,
                other => panic!("unknown param {other}"),
            }
        }
        p
    }

    #[test]
    fn test_parse_ts_bound_unix_seconds() {
        assert_eq!(parse_ts_bound("1769505562", false).unwrap(), 1769505562);
        assert_eq!(parse_ts_bound(" 42 ", true).unwrap(), 42);
    }

    #[test]
    fn test_parse_ts_bound_date() {
        // 2026-01-01T00:00:00Z
        assert_eq!(parse_ts_bound("2026-01-01", false).unwrap(), 1767225600);
        // 2026-01-01T23:59:59Z (inclusive end of day)
        assert_eq!(
            parse_ts_bound("2026-01-01", true).unwrap(),
            1767225600 + 86399
        );
    }

    #[test]
    fn test_parse_ts_bound_invalid() {
        assert!(parse_ts_bound("not-a-date", false).is_err());
        assert!(parse_ts_bound("2026-13-40", false).is_err());
    }

    #[test]
    fn test_filter_blank_params_are_inactive() {
        let filter =
            MemoryFilter::try_from_params(&params(&[("q", "  "), ("source", "")])).unwrap();
        assert!(filter.q.is_none());
        assert!(filter.source.is_none());

        let e = entry("k", "learned", "anything", "src", &[], 1, "");
        assert!(filter.matches(&e));
    }

    #[test]
    fn test_filter_invalid_date_errors() {
        assert!(MemoryFilter::try_from_params(&params(&[("since", "yesterday")])).is_err());
        assert!(MemoryFilter::try_from_params(&params(&[("until", "2026/01/01")])).is_err());
    }

    #[test]
    fn test_filter_query_matches_all_fields_case_insensitive() {
        let e = entry(
            "Dolt-Adapter",
            "learned",
            "Watcher replaced by ADAPTER polling",
            "orchestrator",
            &["Infra"],
            100,
            "sweetgrass-f0j.2",
        );

        for q in ["adapter", "dolt-", "infra", "f0j.2"] {
            let filter = MemoryFilter::try_from_params(&params(&[("q", q)])).unwrap();
            assert!(filter.matches(&e), "query '{q}' should match");
        }

        let filter = MemoryFilter::try_from_params(&params(&[("q", "nomatch")])).unwrap();
        assert!(!filter.matches(&e));
    }

    #[test]
    fn test_filter_type_source_tag() {
        let e = entry(
            "k",
            "investigation",
            "c",
            "detective",
            &["root-cause", "flaky"],
            100,
            "",
        );

        let f = MemoryFilter::try_from_params(&params(&[("type", "investigation")])).unwrap();
        assert!(f.matches(&e));
        let f = MemoryFilter::try_from_params(&params(&[("type", "learned")])).unwrap();
        assert!(!f.matches(&e));

        let f = MemoryFilter::try_from_params(&params(&[("source", "Detective")])).unwrap();
        assert!(f.matches(&e));
        let f = MemoryFilter::try_from_params(&params(&[("source", "orchestrator")])).unwrap();
        assert!(!f.matches(&e));

        let f = MemoryFilter::try_from_params(&params(&[("tag", "FLAKY")])).unwrap();
        assert!(f.matches(&e));
        // Tag filter is exact match, not substring
        let f = MemoryFilter::try_from_params(&params(&[("tag", "root")])).unwrap();
        assert!(!f.matches(&e));
    }

    #[test]
    fn test_filter_date_range_inclusive() {
        let e = entry("k", "learned", "c", "s", &[], 100, "");

        let f = MemoryFilter::try_from_params(&params(&[("since", "100")])).unwrap();
        assert!(f.matches(&e));
        let f = MemoryFilter::try_from_params(&params(&[("since", "101")])).unwrap();
        assert!(!f.matches(&e));

        let f = MemoryFilter::try_from_params(&params(&[("until", "100")])).unwrap();
        assert!(f.matches(&e));
        let f = MemoryFilter::try_from_params(&params(&[("until", "99")])).unwrap();
        assert!(!f.matches(&e));

        let f =
            MemoryFilter::try_from_params(&params(&[("since", "50"), ("until", "150")])).unwrap();
        assert!(f.matches(&e));
    }

    #[test]
    fn test_filter_combined_facets() {
        let a = entry("a", "learned", "uses dolt", "orchestrator", &["db"], 100, "");
        let b = entry("b", "learned", "uses dolt", "detective", &["db"], 100, "");
        let c = entry("c", "learned", "uses jsonl", "orchestrator", &["db"], 100, "");
        let d = entry("d", "learned", "uses dolt", "orchestrator", &["db"], 500, "");

        let f = MemoryFilter::try_from_params(&params(&[
            ("q", "dolt"),
            ("source", "orchestrator"),
            ("tag", "db"),
            ("until", "200"),
        ]))
        .unwrap();

        assert!(f.matches(&a));
        assert!(!f.matches(&b)); // wrong source
        assert!(!f.matches(&c)); // query miss
        assert!(!f.matches(&d)); // outside date range
    }

    #[test]
    fn test_compute_facets() {
        let entries = vec![
            entry("a", "learned", "", "orchestrator", &["db", "infra"], 1, ""),
            entry("b", "learned", "", "detective", &["db"], 2, ""),
            entry("c", "investigation", "", "orchestrator", &[], 3, ""),
        ];

        let facets = compute_facets(&entries);
        assert_eq!(facets.types.get("learned"), Some(&2));
        assert_eq!(facets.types.get("investigation"), Some(&1));
        assert_eq!(facets.sources.get("orchestrator"), Some(&2));
        assert_eq!(facets.sources.get("detective"), Some(&1));
        assert_eq!(facets.tags.get("db"), Some(&2));
        assert_eq!(facets.tags.get("infra"), Some(&1));
    }

    #[test]
    fn test_compute_facets_empty() {
        let facets = compute_facets(&[]);
        assert!(facets.types.is_empty());
        assert!(facets.sources.is_empty());
        assert!(facets.tags.is_empty());
    }
}
