//! Beads API route handlers.
//!
//! Provides endpoints for reading and modifying beads from .beads/issues.jsonl files.

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::validate_path_security;

/// Resolves the correct path to `issues.jsonl` for a project.
///
/// When a project has `sync-branch` set in `.beads/config.yaml`, the canonical
/// JSONL file lives at `.git/beads-worktrees/<branch>/.beads/issues.jsonl`
/// instead of the default `.beads/issues.jsonl`.
///
/// # Fallback behavior
///
/// Returns the default `.beads/issues.jsonl` path when:
/// - No `.beads/config.yaml` exists
/// - The YAML is malformed or cannot be parsed
/// - `sync-branch` is not set, empty, or commented out
/// - The resolved worktree directory does not exist
pub fn resolve_issues_path(project_path: &Path) -> PathBuf {
    let config_path = project_path.join(".beads").join("config.yaml");
    let default_path = project_path.join(".beads").join("issues.jsonl");

    let config_contents = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return default_path,
    };

    let yaml: serde_yaml::Value = match serde_yaml::from_str(&config_contents) {
        Ok(v) => v,
        Err(_) => return default_path,
    };

    let branch = match yaml.get("sync-branch").and_then(|v| v.as_str()) {
        Some(b) if !b.trim().is_empty() => b.trim().to_string(),
        _ => return default_path,
    };

    let worktree_dir = project_path
        .join(".git")
        .join("beads-worktrees")
        .join(&branch);

    if !worktree_dir.exists() {
        return default_path;
    }

    worktree_dir.join(".beads").join("issues.jsonl")
}

/// Query parameters for the beads endpoint.
#[derive(Debug, Deserialize)]
pub struct BeadsParams {
    /// The project path containing .beads/issues.jsonl
    pub path: String,
}

/// A dependency relationship in the JSONL file.
#[derive(Debug, Deserialize, Clone)]
struct Dependency {
    depends_on_id: String,
    #[serde(rename = "type")]
    dep_type: String,
}

/// A single bead/issue from the JSONL file.
#[derive(Debug, Serialize, Deserialize)]
pub struct Bead {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: String,
    // `bd list --json` emits priority as a string ("0"), while the JSONL export
    // emits it as a number; accept either.
    #[serde(default, deserialize_with = "deserialize_priority")]
    pub priority: Option<i32>,
    #[serde(default)]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub close_reason: Option<String>,
    #[serde(default)]
    pub comments: Option<Vec<Comment>>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub children: Option<Vec<String>>,
    #[serde(default, alias = "design")]
    pub design_doc: Option<String>,
    #[serde(default)]
    pub deps: Option<Vec<String>>,
    #[serde(default)]
    pub relates_to: Option<Vec<String>>,
    #[serde(default, skip_serializing)]
    dependencies: Option<Vec<Dependency>>,
}

/// A comment on a bead.
#[derive(Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    pub issue_id: String,
    pub author: String,
    pub text: String,
    pub created_at: String,
}

/// Deserialize a priority that may arrive as a JSON number (JSONL export) or a
/// JSON string (`bd list --json` emits "0".."4").
fn deserialize_priority<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrInt {
        Int(i32),
        Str(String),
        Null,
    }
    match Option::<StrOrInt>::deserialize(deserializer)? {
        None | Some(StrOrInt::Null) => Ok(None),
        Some(StrOrInt::Int(n)) => Ok(Some(n)),
        Some(StrOrInt::Str(s)) => {
            let t = s.trim().trim_start_matches(['p', 'P']);
            Ok(t.parse::<i32>().ok())
        }
    }
}

/// Load the full bead ledger from the beads Dolt DB via `bd list --all --json`.
///
/// This is the source of truth and is always complete and current, unlike the
/// passive `.beads/issues.jsonl` export. Returns an error if `bd` is not on PATH,
/// the command fails, or its output can't be parsed — callers fall back to the
/// JSONL file in that case.
fn read_beads_from_bd(project_path: &Path) -> Result<Vec<Bead>, String> {
    use std::process::Command;

    let output = Command::new("bd")
        .args(["list", "--all", "--json"])
        .current_dir(project_path)
        .output()
        .map_err(|e| format!("failed to spawn bd: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "bd exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let beads: Vec<Bead> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse bd JSON: {}", e))?;
    Ok(beads)
}

/// GET /api/beads?path=/path/to/project
///
/// Reads the full bead ledger from the beads Dolt DB (`bd list --all --json`),
/// falling back to the passive `.beads/issues.jsonl` export when bd is
/// unavailable.
pub async fn read_beads(Query(params): Query<BeadsParams>) -> impl IntoResponse {
    let project_path = PathBuf::from(&params.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": e })),
        );
    }

    // Serve straight from the short-TTL cache when fresh: rapid UI polls
    // (SSE-triggered refetches, several views mounting at once) must not each
    // spawn a `bd` process on large repos (sweetgrass-8u4).
    if let Some(cached) = crate::bd_cache::global().get(&project_path) {
        tracing::debug!("Serving beads for {} from cache", project_path.display());
        return (StatusCode::OK, Json((*cached).clone()));
    }

    // Source of truth is the beads Dolt DB, read via `bd list --all --json`.
    // The .beads/issues.jsonl file is only a passive export that working agents
    // continuously rewrite to a working-set view (open/in_progress-heavy, most
    // closed omitted), so reading it makes the dashboard stale/partial. We shell
    // out to bd for the complete, always-current ledger, and fall back to the
    // JSONL file only when bd is unavailable.
    let (raw_beads, source, bd_schema_version) = match read_beads_from_bd(&project_path) {
        Ok(bs) => {
            tracing::debug!("Loaded {} beads from `bd list --all --json`", bs.len());
            // bd just ran successfully, so this is served from cache after the
            // first call; carry its schema_version so a bump is detectable.
            let schema = super::contract::bd_version_info().schema_version;
            (bs, BeadsSource::Bd, schema)
        }
        Err(bd_err) => {
            tracing::warn!(
                "bd unavailable ({}); falling back to .beads/issues.jsonl",
                bd_err
            );
            let issues_path = resolve_issues_path(&project_path);
            if !issues_path.exists() {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "bd unavailable and no .beads/issues.jsonl found at the specified path"
                    })),
                );
            }
            let contents = match std::fs::read_to_string(&issues_path) {
                Ok(c) => c,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": format!("Failed to read file: {}", e) })),
                    );
                }
            };
            let mut fallback = Vec::new();
            for (line_num, line) in contents.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Bead>(line) {
                    Ok(bead) => fallback.push(bead),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse bead at line {}: {} - {}",
                            line_num + 1,
                            e,
                            line
                        );
                    }
                }
            }
            // The passive JSONL export carries no schema version.
            (fallback, BeadsSource::Jsonl, None)
        }
    };

    let mut beads = raw_beads;
    map_dependencies(&mut beads);

    // Build the versioned envelope and cache it so rapid UI polls are served
    // from memory without re-spawning `bd` (sweetgrass-8u4). The cache holds the
    // full envelope, so cache hits (line ~201) return the same shape as misses.
    let body = beads_envelope(&beads, source, bd_schema_version);
    crate::bd_cache::global().put(&project_path, std::sync::Arc::new(body.clone()));
    (StatusCode::OK, Json(body))
}

/// Where a beads read was served from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BeadsSource {
    /// The bd CLI over the Dolt DB — the source of truth.
    Bd,
    /// The passive `.beads/issues.jsonl` export — fallback only.
    Jsonl,
}

impl BeadsSource {
    fn as_str(self) -> &'static str {
        match self {
            BeadsSource::Bd => "bd",
            BeadsSource::Jsonl => "jsonl",
        }
    }
}

/// Build the versioned response envelope for GET /api/beads.
///
/// Alongside the beads, the envelope carries the API contract version, the
/// source the ledger was read from, and the bd schema_version that produced
/// it (null on the JSONL fallback) — so consumers can detect contract or
/// schema skew instead of breaking silently. All fields are additive over the
/// original `{ "beads": [...] }` shape.
fn beads_envelope(
    beads: &[Bead],
    source: BeadsSource,
    bd_schema_version: Option<u64>,
) -> serde_json::Value {
    serde_json::json!({
        "api_version": super::contract::API_CONTRACT_VERSION,
        "source": source.as_str(),
        "bd_schema_version": bd_schema_version,
        "beads": beads,
    })
}

/// Post-process raw beads: map the `dependencies` array (emitted by both
/// `bd list --all --json` and the JSONL export) onto the UI-facing fields.
///
/// - `parent-child` -> `parent_id` on the child and `children` on the parent,
///   plus ID-pattern inference (e.g. "64n.1" -> parent "64n")
/// - `relates-to` -> `relates_to`
/// - `blocks` -> `deps` (the frontend treats a bead as blocked when `deps`
///   is non-empty)
fn map_dependencies(beads: &mut [Bead]) {
    // Build a map of parent_id -> Vec<child_id>
    let mut parent_to_children: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    // First pass: Extract parent-child relationships from explicit dependencies
    for bead in beads.iter_mut() {
        if let Some(deps) = &bead.dependencies {
            for dep in deps {
                if dep.dep_type == "parent-child" {
                    // Set parent_id on this bead
                    bead.parent_id = Some(dep.depends_on_id.clone());
                    // Record this bead as a child of the parent
                    parent_to_children
                        .entry(dep.depends_on_id.clone())
                        .or_default()
                        .push(bead.id.clone());
                }
            }
        }
    }

    // Second pass: Infer parent-child from ID patterns (e.g., "64n.1" -> parent "64n")
    // This matches how the bd CLI infers relationships when parent_id is not set
    // Collect existing bead IDs first to avoid borrow issues
    let bead_ids: std::collections::HashSet<String> =
        beads.iter().map(|b| b.id.clone()).collect();

    // Collect inferred relationships: (child_id, parent_id)
    let inferred: Vec<(String, String)> = beads
        .iter()
        .filter_map(|bead| {
            // Only infer if parent_id is not already set
            if bead.parent_id.is_some() {
                return None;
            }
            // Check if ID contains a dot (indicating potential child)
            let dot_pos = bead.id.rfind('.')?;
            let potential_parent = &bead.id[..dot_pos];
            // Only infer if the parent exists
            if bead_ids.contains(potential_parent) {
                Some((bead.id.clone(), potential_parent.to_string()))
            } else {
                None
            }
        })
        .collect();

    // Apply inferred relationships
    for (child_id, inferred_parent_id) in &inferred {
        // Set parent_id on the child bead
        if let Some(bead) = beads.iter_mut().find(|b| &b.id == child_id) {
            bead.parent_id = Some(inferred_parent_id.clone());
        }
        // Record in parent_to_children map
        parent_to_children
            .entry(inferred_parent_id.clone())
            .or_default()
            .push(child_id.clone());
    }

    // Third pass: Set children on parent beads
    for bead in beads.iter_mut() {
        if let Some(children) = parent_to_children.get(&bead.id) {
            bead.children = Some(children.clone());
        }
    }

    // Fourth pass: Extract relates-to dependencies into relates_to field
    for bead in beads.iter_mut() {
        if let Some(deps) = &bead.dependencies {
            let related: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "relates-to")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !related.is_empty() {
                bead.relates_to = Some(related);
            }
        }
    }

    // Fifth pass: Extract blocking dependencies into the deps field.
    // The frontend treats a bead as blocked when `deps` is non-empty
    // (see isBlocked in bead-card.tsx), so `blocks`-type dependencies must
    // be surfaced here. Without this, blocking relationships from the JSONL
    // are silently dropped and no bead ever shows as blocked.
    for bead in beads.iter_mut() {
        if let Some(deps) = &bead.dependencies {
            let blocking: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "blocks")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !blocking.is_empty() {
                bead.deps = Some(blocking);
            }
        }
    }
}

/// Request body for adding a comment to a bead.
#[derive(Debug, Deserialize)]
pub struct AddCommentRequest {
    /// The project path containing .beads/issues.jsonl
    pub path: String,
    /// The ID of the bead to add a comment to
    pub bead_id: String,
    /// The comment text
    pub text: String,
    /// The author of the comment (e.g., email address)
    pub author: String,
}

/// Response for the add comment endpoint.
#[derive(Debug, Serialize)]
pub struct AddCommentResponse {
    pub success: bool,
    pub bead: Option<Bead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/beads/comment
///
/// Adds a comment to a specific bead in the .beads/issues.jsonl file.
pub async fn add_comment(Json(payload): Json<AddCommentRequest>) -> impl IntoResponse {
    let project_path = PathBuf::from(&payload.path);

    // Security: Validate path is within allowed directories
    if let Err(e) = validate_path_security(&project_path) {
        return (
            StatusCode::FORBIDDEN,
            Json(AddCommentResponse {
                success: false,
                bead: None,
                error: Some(e),
            }),
        );
    }

    let issues_path = resolve_issues_path(&project_path);

    // Check if the file exists
    if !issues_path.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(AddCommentResponse {
                success: false,
                bead: None,
                error: Some("No .beads/issues.jsonl found at the specified path".to_string()),
            }),
        );
    }

    // Read the file contents
    let contents = match std::fs::read_to_string(&issues_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AddCommentResponse {
                    success: false,
                    bead: None,
                    error: Some(format!("Failed to read file: {}", e)),
                }),
            );
        }
    };

    // Parse JSONL and find the target bead
    let mut beads: Vec<Bead> = Vec::new();
    let mut found_bead_index: Option<usize> = None;
    let mut max_comment_id: i64 = 0;

    for (line_num, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<Bead>(line) {
            Ok(bead) => {
                // Track the maximum comment ID across all beads
                if let Some(comments) = &bead.comments {
                    for comment in comments {
                        if comment.id > max_comment_id {
                            max_comment_id = comment.id;
                        }
                    }
                }

                if bead.id == payload.bead_id {
                    found_bead_index = Some(beads.len());
                }
                beads.push(bead);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse bead at line {}: {} - {}",
                    line_num + 1,
                    e,
                    line
                );
                // Continue parsing other lines - graceful handling of malformed lines
            }
        }
    }

    // Check if the bead was found
    let bead_index = match found_bead_index {
        Some(idx) => idx,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(AddCommentResponse {
                    success: false,
                    bead: None,
                    error: Some(format!("Bead with id '{}' not found", payload.bead_id)),
                }),
            );
        }
    };

    // Create the new comment
    let new_comment = Comment {
        id: max_comment_id + 1,
        issue_id: payload.bead_id.clone(),
        author: payload.author,
        text: payload.text,
        created_at: Utc::now().to_rfc3339(),
    };

    // Add the comment to the bead
    let bead = &mut beads[bead_index];
    match &mut bead.comments {
        Some(comments) => comments.push(new_comment),
        None => bead.comments = Some(vec![new_comment]),
    }

    // Write the updated beads back to the file
    let file = match std::fs::File::create(&issues_path) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AddCommentResponse {
                    success: false,
                    bead: None,
                    error: Some(format!("Failed to open file for writing: {}", e)),
                }),
            );
        }
    };

    let mut writer = std::io::BufWriter::new(file);
    for bead in &beads {
        match serde_json::to_string(bead) {
            Ok(json_line) => {
                if let Err(e) = writeln!(writer, "{}", json_line) {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AddCommentResponse {
                            success: false,
                            bead: None,
                            error: Some(format!("Failed to write to file: {}", e)),
                        }),
                    );
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AddCommentResponse {
                        success: false,
                        bead: None,
                        error: Some(format!("Failed to serialize bead: {}", e)),
                    }),
                );
            }
        }
    }

    if let Err(e) = writer.flush() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AddCommentResponse {
                success: false,
                bead: None,
                error: Some(format!("Failed to flush file: {}", e)),
            }),
        );
    }

    // The ledger changed on disk; drop any cached /api/beads response so the
    // UI's follow-up refetch sees the new comment immediately.
    crate::bd_cache::global().invalidate(&project_path);

    // Return the updated bead
    let updated_bead = beads.swap_remove(bead_index);
    (
        StatusCode::OK,
        Json(AddCommentResponse {
            success: true,
            bead: Some(updated_bead),
            error: None,
        }),
    )
}

/// Computes the appropriate status for an epic based on its children's statuses.
///
/// State machine:
/// - Any child `in_progress` -> Epic `in_progress`
/// - All children `inreview` OR `closed` (with at least one `inreview`) -> Epic `inreview`
/// - All children `open` -> Epic `open`
/// - Note: We don't auto-close epics - user must close manually
fn compute_epic_status_from_children(child_statuses: &[&str]) -> Option<&'static str> {
    if child_statuses.is_empty() {
        return None;
    }

    // Check if any child is in_progress
    if child_statuses.contains(&"in_progress") {
        return Some("in_progress");
    }

    // Check if all children are either inreview or closed
    let all_inreview_or_closed = child_statuses
        .iter()
        .all(|s| *s == "inreview" || *s == "closed");

    if all_inreview_or_closed {
        return Some("inreview");
    }

    // Check if all children are open
    if child_statuses.iter().all(|s| *s == "open") {
        return Some("open");
    }

    // Mixed state (some open, some closed, no in_progress or inreview)
    // Don't change the epic status
    None
}

/// Recomputes and updates epic statuses based on their children's statuses.
///
/// This function reads the issues.jsonl file, finds all epics with children,
/// computes the appropriate status for each epic based on its children,
/// and writes back the file if any epic status changed.
///
/// # Arguments
///
/// * `issues_path` - Path to the .beads/issues.jsonl file
///
/// # Returns
///
/// * `Ok(Vec<String>)` - List of epic IDs that were updated
/// * `Err(String)` - Error message if something went wrong
pub fn recompute_epic_statuses(issues_path: &Path) -> Result<Vec<String>, String> {
    // Read the file contents
    let contents = std::fs::read_to_string(issues_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse JSONL into beads
    let mut beads: Vec<Bead> = Vec::new();
    for (line_num, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<Bead>(line) {
            Ok(bead) => beads.push(bead),
            Err(e) => {
                tracing::warn!(
                    "Failed to parse bead at line {}: {} - {}",
                    line_num + 1,
                    e,
                    line
                );
            }
        }
    }

    // Build parent-child relationships
    // parent_id -> Vec<child_id>
    let mut parent_to_children: HashMap<String, Vec<String>> = HashMap::new();

    // First pass: Extract from explicit dependencies
    for bead in &beads {
        if let Some(deps) = &bead.dependencies {
            for dep in deps {
                if dep.dep_type == "parent-child" {
                    parent_to_children
                        .entry(dep.depends_on_id.clone())
                        .or_default()
                        .push(bead.id.clone());
                }
            }
        }
    }

    // Second pass: Infer parent-child from ID patterns (e.g., "64n.1" -> parent "64n")
    // This matches how the bd CLI infers relationships when parent_id is not set
    let bead_ids: std::collections::HashSet<String> =
        beads.iter().map(|b| b.id.clone()).collect();

    for bead in &beads {
        // Only infer if not already in parent_to_children
        if bead.parent_id.is_none() && bead.id.contains('.') {
            if let Some(dot_pos) = bead.id.rfind('.') {
                let potential_parent = &bead.id[..dot_pos];
                // Only infer if parent exists and not already recorded
                if bead_ids.contains(potential_parent) {
                    let children = parent_to_children
                        .entry(potential_parent.to_string())
                        .or_default();
                    if !children.contains(&bead.id) {
                        children.push(bead.id.clone());
                    }
                }
            }
        }
    }

    // Build a map of bead_id -> status for quick lookup (owned strings to avoid borrow issues)
    let status_map: HashMap<String, String> = beads
        .iter()
        .map(|b| (b.id.clone(), b.status.clone()))
        .collect();

    // First pass: find which epics need updates
    let mut epic_updates: Vec<(String, String)> = Vec::new();

    for bead in &beads {
        // Only process epics
        if bead.issue_type.as_deref() != Some("epic") {
            continue;
        }

        // Skip closed epics - don't auto-reopen them
        if bead.status == "closed" {
            continue;
        }

        // Get children for this epic
        let children = match parent_to_children.get(&bead.id) {
            Some(c) => c,
            None => continue, // No children, skip
        };

        // Collect child statuses
        let child_statuses: Vec<&str> = children
            .iter()
            .filter_map(|child_id| status_map.get(child_id).map(String::as_str))
            .collect();

        // Compute the new status
        if let Some(new_status) = compute_epic_status_from_children(&child_statuses) {
            if bead.status != new_status {
                epic_updates.push((bead.id.clone(), new_status.to_string()));
            }
        }
    }

    // Second pass: apply updates
    let mut updated_epic_ids: Vec<String> = Vec::new();

    for (epic_id, new_status) in &epic_updates {
        for bead in &mut beads {
            if &bead.id == epic_id {
                tracing::info!(
                    "Updating epic {} status from {} to {}",
                    bead.id,
                    bead.status,
                    new_status
                );
                bead.status = new_status.clone();
                bead.updated_at = Some(Utc::now().to_rfc3339());
                updated_epic_ids.push(bead.id.clone());
                break;
            }
        }
    }

    // Write back if any epic was updated
    if !updated_epic_ids.is_empty() {
        let file = std::fs::File::create(issues_path)
            .map_err(|e| format!("Failed to open file for writing: {}", e))?;

        let mut writer = std::io::BufWriter::new(file);
        for bead in &beads {
            let json_line = serde_json::to_string(bead)
                .map_err(|e| format!("Failed to serialize bead: {}", e))?;
            writeln!(writer, "{}", json_line)
                .map_err(|e| format!("Failed to write to file: {}", e))?;
        }
        writer
            .flush()
            .map_err(|e| format!("Failed to flush file: {}", e))?;
    }

    Ok(updated_epic_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bead() {
        let json = r#"{"id":"test-123","title":"Test Bead","status":"open","priority":2}"#;
        let bead: Bead = serde_json::from_str(json).unwrap();
        assert_eq!(bead.id, "test-123");
        assert_eq!(bead.title, "Test Bead");
        assert_eq!(bead.status, "open");
        assert_eq!(bead.priority, Some(2));
    }

    #[test]
    fn test_parse_bead_with_comments() {
        let json = r#"{"id":"test-456","title":"With Comments","status":"closed","comments":[{"id":1,"issue_id":"test-456","author":"user","text":"A comment","created_at":"2026-01-01T00:00:00Z"}]}"#;
        let bead: Bead = serde_json::from_str(json).unwrap();
        assert_eq!(bead.comments.as_ref().unwrap().len(), 1);
        assert_eq!(bead.comments.as_ref().unwrap()[0].text, "A comment");
    }

    #[test]
    fn test_parse_bead_with_design_field() {
        // Test that alias "design" works
        let json = r#"{"id":"test-789","title":"With Design","status":"open","design":"path/to/design.md"}"#;
        let bead: Bead = serde_json::from_str(json).unwrap();
        assert_eq!(bead.design_doc, Some("path/to/design.md".to_string()));
    }

    #[test]
    fn test_parse_bead_with_design_doc_field() {
        // Test that original "design_doc" still works
        let json = r#"{"id":"test-790","title":"With Design Doc","status":"open","design_doc":"path/to/design2.md"}"#;
        let bead: Bead = serde_json::from_str(json).unwrap();
        assert_eq!(bead.design_doc, Some("path/to/design2.md".to_string()));
    }

    #[test]
    fn test_compute_epic_status_any_in_progress() {
        // Any child in_progress -> Epic in_progress
        let statuses = vec!["open", "in_progress", "closed"];
        assert_eq!(
            compute_epic_status_from_children(&statuses),
            Some("in_progress")
        );
    }

    #[test]
    fn test_compute_epic_status_all_open() {
        // All children open -> Epic open
        let statuses = vec!["open", "open", "open"];
        assert_eq!(compute_epic_status_from_children(&statuses), Some("open"));
    }

    #[test]
    fn test_compute_epic_status_all_inreview_or_closed_with_inreview() {
        // All children inreview or closed (with at least one inreview) -> Epic inreview
        let statuses = vec!["inreview", "closed", "inreview"];
        assert_eq!(
            compute_epic_status_from_children(&statuses),
            Some("inreview")
        );
    }

    #[test]
    fn test_compute_epic_status_all_closed() {
        // All children closed -> Epic should be inreview (ready for final review)
        let statuses = vec!["closed", "closed"];
        assert_eq!(compute_epic_status_from_children(&statuses), Some("inreview"));
    }

    #[test]
    fn test_compute_epic_status_mixed_open_closed() {
        // Mixed open and closed (no in_progress or inreview) -> No change
        let statuses = vec!["open", "closed"];
        assert_eq!(compute_epic_status_from_children(&statuses), None);
    }

    #[test]
    fn test_compute_epic_status_empty() {
        // No children -> No change
        let statuses: Vec<&str> = vec![];
        assert_eq!(compute_epic_status_from_children(&statuses), None);
    }

    #[test]
    fn test_compute_epic_status_single_in_progress() {
        let statuses = vec!["in_progress"];
        assert_eq!(
            compute_epic_status_from_children(&statuses),
            Some("in_progress")
        );
    }

    #[test]
    fn test_compute_epic_status_single_inreview() {
        let statuses = vec!["inreview"];
        assert_eq!(
            compute_epic_status_from_children(&statuses),
            Some("inreview")
        );
    }

    #[test]
    fn test_infer_parent_from_id_pattern() {
        // Test the ID pattern inference logic
        // Bead "64n.1" should be inferred as child of "64n" if parent exists
        let bead_id = "64n.1";
        let dot_pos = bead_id.rfind('.');
        assert!(dot_pos.is_some());
        let parent_id = &bead_id[..dot_pos.unwrap()];
        assert_eq!(parent_id, "64n");
    }

    #[test]
    fn test_infer_parent_multiple_dots() {
        // Test that we extract the correct parent when ID has multiple dots
        // Bead "prefix.64n.1" should have parent "prefix.64n"
        let bead_id = "prefix.64n.1";
        let dot_pos = bead_id.rfind('.');
        assert!(dot_pos.is_some());
        let parent_id = &bead_id[..dot_pos.unwrap()];
        assert_eq!(parent_id, "prefix.64n");
    }

    #[test]
    fn test_no_inference_without_dot() {
        // Bead without dot should not have inferred parent
        let bead_id = "simple-id";
        let dot_pos = bead_id.rfind('.');
        assert!(dot_pos.is_none());
    }

    #[test]
    fn test_parse_bead_with_relates_to_dependencies() {
        // Test that relates-to dependencies are deserialized from the dependencies array
        let json = r#"{"id":"bead-a","title":"Bead A","status":"open","dependencies":[{"issue_id":"bead-a","depends_on_id":"bead-b","type":"relates-to","created_at":"2026-01-27T00:00:00Z","created_by":"user"},{"issue_id":"bead-a","depends_on_id":"bead-c","type":"relates-to","created_at":"2026-01-27T00:00:00Z","created_by":"user"}]}"#;
        let bead: Bead = serde_json::from_str(json).unwrap();
        let deps = bead.dependencies.unwrap();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].dep_type, "relates-to");
        assert_eq!(deps[0].depends_on_id, "bead-b");
        assert_eq!(deps[1].dep_type, "relates-to");
        assert_eq!(deps[1].depends_on_id, "bead-c");
    }

    #[test]
    fn test_relates_to_extraction_logic() {
        // Test the fourth pass extraction logic that populates relates_to from dependencies
        let mut bead = Bead {
            id: "bead-a".to_string(),
            title: "Bead A".to_string(),
            description: None,
            status: "open".to_string(),
            priority: None,
            issue_type: None,
            owner: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            close_reason: None,
            comments: None,
            parent_id: None,
            children: None,
            design_doc: None,
            deps: None,
            relates_to: None,
            dependencies: Some(vec![
                Dependency {
                    depends_on_id: "bead-b".to_string(),
                    dep_type: "relates-to".to_string(),
                },
                Dependency {
                    depends_on_id: "bead-parent".to_string(),
                    dep_type: "parent-child".to_string(),
                },
                Dependency {
                    depends_on_id: "bead-c".to_string(),
                    dep_type: "relates-to".to_string(),
                },
            ]),
        };

        // Simulate the fourth pass extraction logic
        if let Some(deps) = &bead.dependencies {
            let related: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "relates-to")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !related.is_empty() {
                bead.relates_to = Some(related);
            }
        }

        // Verify only relates-to deps are extracted, not parent-child
        let relates_to = bead.relates_to.unwrap();
        assert_eq!(relates_to.len(), 2);
        assert_eq!(relates_to[0], "bead-b");
        assert_eq!(relates_to[1], "bead-c");
    }

    #[test]
    fn test_blocks_extraction_into_deps() {
        // Test the fifth pass: blocks-type dependencies populate `deps`,
        // while parent-child and relates-to are excluded.
        let mut bead = Bead {
            id: "bead-a".to_string(),
            title: "Bead A".to_string(),
            description: None,
            status: "open".to_string(),
            priority: None,
            issue_type: None,
            owner: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            close_reason: None,
            comments: None,
            parent_id: None,
            children: None,
            design_doc: None,
            deps: None,
            relates_to: None,
            dependencies: Some(vec![
                Dependency {
                    depends_on_id: "bead-blocker".to_string(),
                    dep_type: "blocks".to_string(),
                },
                Dependency {
                    depends_on_id: "bead-parent".to_string(),
                    dep_type: "parent-child".to_string(),
                },
                Dependency {
                    depends_on_id: "bead-related".to_string(),
                    dep_type: "relates-to".to_string(),
                },
            ]),
        };

        // Simulate the fifth pass extraction logic
        if let Some(deps) = &bead.dependencies {
            let blocking: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "blocks")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !blocking.is_empty() {
                bead.deps = Some(blocking);
            }
        }

        // Only the blocks dep should be surfaced in deps
        let deps = bead.deps.unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], "bead-blocker");
    }

    #[test]
    fn test_deps_none_when_no_blocks_deps() {
        // deps stays None when there are no blocks-type dependencies
        let mut bead = Bead {
            id: "bead-x".to_string(),
            title: "Bead X".to_string(),
            description: None,
            status: "open".to_string(),
            priority: None,
            issue_type: None,
            owner: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            close_reason: None,
            comments: None,
            parent_id: None,
            children: None,
            design_doc: None,
            deps: None,
            relates_to: None,
            dependencies: Some(vec![Dependency {
                depends_on_id: "bead-parent".to_string(),
                dep_type: "parent-child".to_string(),
            }]),
        };

        if let Some(deps) = &bead.dependencies {
            let blocking: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "blocks")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !blocking.is_empty() {
                bead.deps = Some(blocking);
            }
        }

        assert!(bead.deps.is_none());
    }

    #[test]
    fn test_relates_to_none_when_no_relates_to_deps() {
        // Test that relates_to stays None when there are no relates-to dependencies
        let mut bead = Bead {
            id: "bead-x".to_string(),
            title: "Bead X".to_string(),
            description: None,
            status: "open".to_string(),
            priority: None,
            issue_type: None,
            owner: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            close_reason: None,
            comments: None,
            parent_id: None,
            children: None,
            design_doc: None,
            deps: None,
            relates_to: None,
            dependencies: Some(vec![Dependency {
                depends_on_id: "bead-parent".to_string(),
                dep_type: "parent-child".to_string(),
            }]),
        };

        // Simulate the fourth pass extraction logic
        if let Some(deps) = &bead.dependencies {
            let related: Vec<String> = deps
                .iter()
                .filter(|dep| dep.dep_type == "relates-to")
                .map(|dep| dep.depends_on_id.clone())
                .collect();
            if !related.is_empty() {
                bead.relates_to = Some(related);
            }
        }

        // No relates-to deps, so relates_to should remain None
        assert!(bead.relates_to.is_none());
    }

    #[test]
    fn test_relates_to_serialized_in_json() {
        // Test that relates_to is included in serialized JSON output
        // (unlike dependencies which has skip_serializing)
        let bead = Bead {
            id: "bead-s".to_string(),
            title: "Serialization Test".to_string(),
            description: None,
            status: "open".to_string(),
            priority: None,
            issue_type: None,
            owner: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            close_reason: None,
            comments: None,
            parent_id: None,
            children: None,
            design_doc: None,
            deps: None,
            relates_to: Some(vec!["bead-r1".to_string(), "bead-r2".to_string()]),
            dependencies: Some(vec![Dependency {
                depends_on_id: "bead-r1".to_string(),
                dep_type: "relates-to".to_string(),
            }]),
        };

        let json = serde_json::to_string(&bead).unwrap();

        // relates_to SHOULD be serialized
        assert!(json.contains("relates_to"));
        assert!(json.contains("bead-r1"));
        assert!(json.contains("bead-r2"));

        // dependencies should NOT be serialized (skip_serializing)
        assert!(!json.contains("dependencies"));
        assert!(!json.contains("depends_on_id"));
    }

    // ── resolve_issues_path tests ──────────────────────────────────────

    #[test]
    fn test_resolve_no_config_file() {
        // When .beads/config.yaml does not exist, fall back to default
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join(".beads")).unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_empty_config_file() {
        // Empty config file -> default path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(beads_dir.join("config.yaml"), "").unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_commented_out_sync_branch() {
        // sync-branch is commented out -> default path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "# sync-branch: \"beads-sync\"\n",
        )
        .unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_valid_sync_branch() {
        // Valid sync-branch with existing worktree dir -> sync path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();

        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: \"beads-sync\"\n",
        )
        .unwrap();

        // Create the worktree directory
        let worktree_beads = project
            .join(".git")
            .join("beads-worktrees")
            .join("beads-sync")
            .join(".beads");
        std::fs::create_dir_all(&worktree_beads).unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, worktree_beads.join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_malformed_yaml() {
        // Malformed YAML -> default path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: [invalid: yaml: {{\n",
        )
        .unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_worktree_dir_missing() {
        // sync-branch set but worktree directory does not exist -> default
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: \"nonexistent-branch\"\n",
        )
        .unwrap();
        // Do NOT create .git/beads-worktrees/nonexistent-branch

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_spaces_in_branch_name() {
        // Branch name with spaces (unusual but valid YAML string)
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: \"my branch\"\n",
        )
        .unwrap();

        let worktree_dir = project
            .join(".git")
            .join("beads-worktrees")
            .join("my branch");
        std::fs::create_dir_all(&worktree_dir).unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(
            result,
            worktree_dir.join(".beads").join("issues.jsonl")
        );
    }

    #[test]
    fn test_resolve_empty_string_sync_branch() {
        // sync-branch set to empty string -> default path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: \"\"\n",
        )
        .unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_sync_branch_without_quotes() {
        // YAML allows unquoted strings
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: beads-sync\n",
        )
        .unwrap();

        let worktree_beads = project
            .join(".git")
            .join("beads-worktrees")
            .join("beads-sync")
            .join(".beads");
        std::fs::create_dir_all(&worktree_beads).unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, worktree_beads.join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_sync_branch_with_other_keys() {
        // Config has other keys alongside sync-branch
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "issue-prefix: myproject\nsync-branch: beads-sync\nno-db: true\n",
        )
        .unwrap();

        let worktree_beads = project
            .join(".git")
            .join("beads-worktrees")
            .join("beads-sync")
            .join(".beads");
        std::fs::create_dir_all(&worktree_beads).unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, worktree_beads.join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_sync_branch_null_value() {
        // sync-branch set to YAML null -> default path
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let beads_dir = project.join(".beads");
        std::fs::create_dir_all(&beads_dir).unwrap();
        std::fs::write(
            beads_dir.join("config.yaml"),
            "sync-branch: null\n",
        )
        .unwrap();

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }

    #[test]
    fn test_resolve_no_beads_dir() {
        // No .beads directory at all -> default path (read fails gracefully)
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        // Do NOT create .beads/

        let result = resolve_issues_path(project);
        assert_eq!(result, project.join(".beads").join("issues.jsonl"));
    }
}

/// Golden-fixture tests: deterministic, committed fixtures under
/// `server/tests/fixtures/` in the two shapes the server ingests —
/// `bd list --all --json` output and the `.beads/issues.jsonl` export.
#[cfg(test)]
mod golden_fixture_tests {
    use super::*;

    /// Golden fixture in `bd list --all --json` shape (priority as string).
    const BD_LIST_ALL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bd_list_all.json"
    ));

    /// Golden fixture in `.beads/issues.jsonl` export shape (priority as int).
    const ISSUES_JSONL: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/issues.jsonl"
    ));

    fn load_bd_fixture() -> Vec<Bead> {
        serde_json::from_str(BD_LIST_ALL).expect("bd_list_all.json must deserialize")
    }

    fn load_jsonl_fixture() -> Vec<Bead> {
        ISSUES_JSONL
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).expect("issues.jsonl line must deserialize"))
            .collect()
    }

    fn find<'a>(beads: &'a [Bead], id: &str) -> &'a Bead {
        beads
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("bead {} missing from fixture", id))
    }

    #[test]
    fn bd_fixture_deserializes_all_issues() {
        let beads = load_bd_fixture();
        assert_eq!(beads.len(), 7);
        for id in [
            "golden-epc",
            "golden-epc.1",
            "golden-chd",
            "golden-blk",
            "golden-rel",
            "golden-nop",
            "orphan.9",
        ] {
            find(&beads, id);
        }
    }

    #[test]
    fn bd_fixture_core_fields() {
        let beads = load_bd_fixture();

        let epic = find(&beads, "golden-epc");
        assert_eq!(epic.title, "Golden fixture root epic");
        assert_eq!(epic.status, "open");
        assert_eq!(epic.issue_type.as_deref(), Some("epic"));
        assert_eq!(epic.owner.as_deref(), Some("fixture@example.com"));
        assert_eq!(epic.created_at.as_deref(), Some("2026-01-01T00:00:00Z"));

        let rel = find(&beads, "golden-rel");
        assert_eq!(rel.status, "closed");
        assert_eq!(rel.closed_at.as_deref(), Some("2026-01-05T00:00:00Z"));
        assert_eq!(rel.close_reason.as_deref(), Some("done in fixture"));
    }

    #[test]
    fn bd_fixture_priority_string_coercion() {
        // `bd list --json` emits priority as a string ("0".."4"); the
        // deserializer must coerce to ints and leave absent values None.
        let beads = load_bd_fixture();
        assert_eq!(find(&beads, "golden-epc").priority, Some(0));
        assert_eq!(find(&beads, "golden-epc.1").priority, Some(1));
        assert_eq!(find(&beads, "golden-chd").priority, Some(2));
        assert_eq!(find(&beads, "golden-blk").priority, Some(1));
        assert_eq!(find(&beads, "golden-rel").priority, Some(3));
        assert_eq!(find(&beads, "orphan.9").priority, Some(4));
        assert_eq!(find(&beads, "golden-nop").priority, None);
    }

    #[test]
    fn bd_fixture_dependency_mapping() {
        let mut beads = load_bd_fixture();
        map_dependencies(&mut beads);

        // Explicit parent-child dependency -> parent_id
        assert_eq!(
            find(&beads, "golden-chd").parent_id.as_deref(),
            Some("golden-epc")
        );
        // ID-pattern inference: golden-epc.1 -> golden-epc
        assert_eq!(
            find(&beads, "golden-epc.1").parent_id.as_deref(),
            Some("golden-epc")
        );
        // Parent collects both children (explicit + inferred)
        let mut children = find(&beads, "golden-epc").children.clone().unwrap();
        children.sort();
        assert_eq!(children, vec!["golden-chd", "golden-epc.1"]);
        // Dotted ID without an existing parent bead gets no inferred parent
        assert!(find(&beads, "orphan.9").parent_id.is_none());

        // blocks -> deps (parent-child and relates-to excluded)
        assert_eq!(
            find(&beads, "golden-blk").deps.as_deref(),
            Some(&["golden-chd".to_string()][..])
        );
        // relates-to -> relates_to (blocks excluded)
        assert_eq!(
            find(&beads, "golden-blk").relates_to.as_deref(),
            Some(&["golden-rel".to_string()][..])
        );
        assert_eq!(
            find(&beads, "golden-rel").relates_to.as_deref(),
            Some(&["golden-blk".to_string()][..])
        );

        // Beads with no dependencies keep None everywhere
        let nop = find(&beads, "golden-nop");
        assert!(nop.deps.is_none());
        assert!(nop.relates_to.is_none());
        assert!(nop.parent_id.is_none());
    }

    #[test]
    fn bd_fixture_serialization_hides_raw_dependencies() {
        let mut beads = load_bd_fixture();
        map_dependencies(&mut beads);
        let json = serde_json::to_string(&beads).unwrap();

        // The raw dependencies array is internal-only (skip_serializing)
        assert!(!json.contains("depends_on_id"));
        // Mapped fields are surfaced to the frontend
        assert!(json.contains("\"parent_id\":\"golden-epc\""));
        assert!(json.contains("\"deps\":[\"golden-chd\"]"));
        assert!(json.contains("\"relates_to\":[\"golden-rel\"]"));
    }

    #[test]
    fn jsonl_fixture_parses_line_by_line() {
        // Exercises the JSONL fallback parse path: per-line Bead deserialization
        let beads = load_jsonl_fixture();
        assert_eq!(beads.len(), 5);

        // The JSONL export emits integer priorities
        assert_eq!(find(&beads, "golden-epc").priority, Some(0));
        assert_eq!(find(&beads, "golden-blk").priority, Some(1));
        // "P2"-style string priorities coerce too
        assert_eq!(find(&beads, "golden-pfx").priority, Some(2));

        // Comments survive deserialization
        let comments = find(&beads, "golden-cmt").comments.as_ref().unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "golden comment");
        assert_eq!(comments[0].author, "fixture@example.com");
    }

    #[test]
    fn beads_envelope_carries_contract_and_schema_version() {
        let mut beads = load_bd_fixture();
        map_dependencies(&mut beads);

        let envelope = beads_envelope(&beads, BeadsSource::Bd, Some(1));

        assert_eq!(
            envelope["api_version"],
            super::super::contract::API_CONTRACT_VERSION
        );
        assert_eq!(envelope["source"], "bd");
        assert_eq!(envelope["bd_schema_version"], 1);
        assert_eq!(envelope["beads"].as_array().unwrap().len(), beads.len());
    }

    #[test]
    fn beads_envelope_jsonl_fallback_has_null_schema() {
        let mut beads = load_jsonl_fixture();
        map_dependencies(&mut beads);

        let envelope = beads_envelope(&beads, BeadsSource::Jsonl, None);

        assert_eq!(
            envelope["api_version"],
            super::super::contract::API_CONTRACT_VERSION
        );
        assert_eq!(envelope["source"], "jsonl");
        assert!(envelope["bd_schema_version"].is_null());
        assert_eq!(envelope["beads"].as_array().unwrap().len(), beads.len());
    }

    #[test]
    fn jsonl_fixture_dependency_mapping_matches_bd_shape() {
        // The same mapping passes apply to the JSONL export shape
        let mut beads = load_jsonl_fixture();
        map_dependencies(&mut beads);

        assert_eq!(
            find(&beads, "golden-chd").parent_id.as_deref(),
            Some("golden-epc")
        );
        assert_eq!(
            find(&beads, "golden-epc").children.as_deref(),
            Some(&["golden-chd".to_string()][..])
        );
        assert_eq!(
            find(&beads, "golden-blk").deps.as_deref(),
            Some(&["golden-chd".to_string()][..])
        );
    }
}
