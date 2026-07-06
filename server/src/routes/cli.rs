//! CLI route handlers for executing bd commands.
//!
//! Provides a secure endpoint for executing whitelisted bd CLI commands.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

/// Whitelisted bd subcommands that are allowed to be executed.
///
/// The last five are the upstream bd KV memory commands (System A): keyed
/// key→string rows stored in the embedded beads DB and injected into agent
/// sessions via `bd prime`. This is a distinct system from the
/// `.beads/memory/knowledge.jsonl` files served by `routes::memory`
/// (System B) — the two must never be conflated.
const ALLOWED_COMMANDS: &[&str] = &[
    "list", "show", "comment", "update", "close", "create", "ready", "epic",
    "remember", "recall", "memories", "forget", "prime",
];

/// Subcommands that never modify the ledger. Any other successful command is
/// treated as a write and invalidates the cached `/api/beads` response for
/// the project it ran in.
const READ_ONLY_COMMANDS: &[&str] = &["list", "show", "ready"];

/// Request body for the bd command endpoint.
#[derive(Deserialize)]
pub struct BdCommandRequest {
    /// Arguments to pass to the bd command.
    pub args: Vec<String>,
    /// Optional working directory for command execution.
    pub cwd: Option<String>,
}

/// Response body for the bd command endpoint.
#[derive(Serialize)]
pub struct BdCommandResponse {
    /// Standard output from the command.
    pub stdout: String,
    /// Standard error from the command.
    pub stderr: String,
    /// Exit code from the command.
    pub code: i32,
}

/// Classified category of a bd command failure, derived from exit code + stderr.
///
/// Turns bd's opaque nonzero exits into a real taxonomy the UI can branch on,
/// instead of a 200 response with the error buried in stderr.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BdErrorKind {
    /// Dolt/DB lock contention or a busy database — retryable.
    Lock,
    /// Merge conflict / diverged chunkstore — needs sync resolution.
    Merge,
    /// Input rejected by bd (bad type, missing required field, cycle).
    Validation,
    /// Referenced issue/epic/dependency does not exist.
    NotFound,
    /// Schema-version mismatch between bd and the DB (fork-risk class).
    SchemaMismatch,
    /// Anything else non-zero.
    Unknown,
}

impl BdErrorKind {
    fn classify(code: i32, stderr: &str) -> Self {
        let e = stderr.to_lowercase();
        if e.contains("database is locked") || e.contains("lock") || e.contains("busy") {
            BdErrorKind::Lock
        } else if e.contains("conflict") || e.contains("merge") || e.contains("diverged") {
            BdErrorKind::Merge
        } else if e.contains("schema") && (e.contains("version") || e.contains("migrat")) {
            BdErrorKind::SchemaMismatch
        } else if e.contains("not found") || e.contains("no such") || e.contains("does not exist")
            || e.contains("no issue found") || e.contains("resolving id") || e.contains("found matching")
            || e.contains("no memory with key") {
            BdErrorKind::NotFound
        } else if e.contains("invalid") || e.contains("required") || e.contains("cycle") || e.contains("must ") {
            BdErrorKind::Validation
        } else {
            let _ = code;
            BdErrorKind::Unknown
        }
    }

    /// HTTP status a given failure category maps to.
    fn status(&self) -> StatusCode {
        match self {
            BdErrorKind::Lock => StatusCode::SERVICE_UNAVAILABLE,      // 503 — retry
            BdErrorKind::Merge => StatusCode::CONFLICT,                // 409
            BdErrorKind::Validation => StatusCode::UNPROCESSABLE_ENTITY, // 422
            BdErrorKind::NotFound => StatusCode::NOT_FOUND,            // 404
            BdErrorKind::SchemaMismatch => StatusCode::CONFLICT,       // 409 — needs migrator
            BdErrorKind::Unknown => StatusCode::INTERNAL_SERVER_ERROR, // 500
        }
    }

    /// Whether a client may safely retry this failure as-is.
    fn retryable(&self) -> bool {
        matches!(self, BdErrorKind::Lock)
    }
}

/// Execute a bd command with the provided arguments.
///
/// # Security
///
/// - Only whitelisted subcommands are allowed
/// - Working directory is validated to exist
/// - Command execution has a 30-second timeout
///
/// # Endpoint
///
/// `POST /api/bd/command`
pub async fn bd_command(Json(req): Json<BdCommandRequest>) -> impl IntoResponse {
    // Validate that we have at least one argument (the subcommand)
    if req.args.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "No arguments provided. Expected a bd subcommand."
            })),
        )
            .into_response();
    }

    // Check if the subcommand is whitelisted
    let subcommand = &req.args[0];
    if !ALLOWED_COMMANDS.contains(&subcommand.as_str()) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!(
                    "Command '{}' is not allowed. Allowed commands: {:?}",
                    subcommand, ALLOWED_COMMANDS
                )
            })),
        )
            .into_response();
    }

    // Validate and set working directory
    let cwd = if let Some(ref dir) = req.cwd {
        let path = Path::new(dir);
        if !path.exists() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Working directory does not exist: {}", dir)
                })),
            )
                .into_response();
        }
        if !path.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Path is not a directory: {}", dir)
                })),
            )
                .into_response();
        }
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
    };

    // Build and execute the command with timeout
    let mut cmd = Command::new("bd");
    cmd.args(&req.args).current_dir(&cwd);

    let result = tokio::time::timeout(Duration::from_secs(30), cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let code = output.status.code().unwrap_or(-1);

            if output.status.success() {
                // A successful write-path command changed the ledger; make sure
                // the next /api/beads read is not served from a stale cache.
                if !READ_ONLY_COMMANDS.contains(&subcommand.as_str()) {
                    crate::bd_cache::global().invalidate(&cwd);
                }
                Json(BdCommandResponse { stdout, stderr, code }).into_response()
            } else {
                // Nonzero exit: classify into a real taxonomy instead of a silent 200.
                let kind = BdErrorKind::classify(code, &stderr);
                let status = kind.status();
                (
                    status,
                    Json(serde_json::json!({
                        "error": stderr.trim(),
                        "kind": kind,
                        "retryable": kind.retryable(),
                        "code": code,
                        "command": subcommand,
                    })),
                )
                    .into_response()
            }
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to execute command: {}", e)
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({
                "error": "Command timed out after 30 seconds"
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_commands_contains_expected() {
        assert!(ALLOWED_COMMANDS.contains(&"list"));
        assert!(ALLOWED_COMMANDS.contains(&"show"));
        assert!(ALLOWED_COMMANDS.contains(&"comment"));
        assert!(ALLOWED_COMMANDS.contains(&"update"));
        assert!(ALLOWED_COMMANDS.contains(&"close"));
        assert!(ALLOWED_COMMANDS.contains(&"create"));
    }

    #[test]
    fn test_read_only_commands_are_a_subset_of_allowed() {
        for cmd in READ_ONLY_COMMANDS {
            assert!(ALLOWED_COMMANDS.contains(cmd));
        }
        // Write-path commands must NOT be in the read-only set, or their
        // effects would be masked by the beads cache for a TTL window.
        for cmd in ["comment", "update", "close", "create", "epic"] {
            assert!(!READ_ONLY_COMMANDS.contains(&cmd));
        }
    }

    #[test]
    fn test_disallowed_commands() {
        assert!(!ALLOWED_COMMANDS.contains(&"rm"));
        assert!(!ALLOWED_COMMANDS.contains(&"delete"));
        assert!(!ALLOWED_COMMANDS.contains(&"exec"));
    }

    /// The upstream bd KV memory commands (System A) must all be allowed so
    /// the UI can list/add/delete memories through /api/bd/command.
    #[test]
    fn test_allowed_commands_contains_kv_memory() {
        assert!(ALLOWED_COMMANDS.contains(&"remember"));
        assert!(ALLOWED_COMMANDS.contains(&"recall"));
        assert!(ALLOWED_COMMANDS.contains(&"memories"));
        assert!(ALLOWED_COMMANDS.contains(&"forget"));
        assert!(ALLOWED_COMMANDS.contains(&"prime"));
    }

    /// `bd recall`/`bd forget` on a missing key report
    /// `No memory with key "<key>"` on stderr — classify as NotFound.
    #[test]
    fn test_classify_missing_memory_key() {
        let kind = BdErrorKind::classify(1, r#"No memory with key "nonexistent""#);
        assert_eq!(kind, BdErrorKind::NotFound);
        assert_eq!(kind.status(), StatusCode::NOT_FOUND);
        assert!(!kind.retryable());
    }
}
