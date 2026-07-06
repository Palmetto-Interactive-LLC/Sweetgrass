//! Versioned API contract between the Sweetgrass UI and this server.
//!
//! Sweetgrass has two version axes that can drift independently, and both
//! must be detectable rather than silently breaking:
//!
//! 1. **API contract version** ([`API_CONTRACT_VERSION`]) — the shape of the
//!    HTTP contract between the embedded UI and this server. Bumped manually
//!    on any breaking change to request/response shapes. The server stamps it
//!    on every response as the [`API_VERSION_HEADER`] header and carries it in
//!    payload envelopes, so a UI built against a different contract detects
//!    the skew instead of failing on a missing/renamed field.
//! 2. **bd schema version** — the beads database schema the local `bd`
//!    binary speaks, read from `bd version --json`. Upstream beads migrates
//!    its schema aggressively, and a mixed-version fleet can permanently fork
//!    a DB (beads #4259 class). We surface the observed schema_version plus
//!    whether it falls inside this server's supported range
//!    ([`SUPPORTED_BD_SCHEMA_MIN`]..=[`SUPPORTED_BD_SCHEMA_MAX`]), so a bump
//!    shows up as an explicit warning, not corrupted rendering.

use axum::{response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Version of the HTTP contract between the UI and this server.
///
/// Bump this (and `EXPECTED_API_VERSION` in `src/lib/api.ts`) together on any
/// breaking change to an API request or response shape. Additive changes do
/// not require a bump.
pub const API_CONTRACT_VERSION: u32 = 1;

/// Response header carrying [`API_CONTRACT_VERSION`] on every response.
///
/// Must be lowercase — it is used with `HeaderName::from_static`.
pub const API_VERSION_HEADER: &str = "x-sweetgrass-api-version";

/// Lowest bd schema_version this server is known to work against.
pub const SUPPORTED_BD_SCHEMA_MIN: u64 = 1;

/// Highest bd schema_version this server is known to work against.
pub const SUPPORTED_BD_SCHEMA_MAX: u64 = 1;

/// Whether a bd schema_version falls in this server's supported range.
pub fn schema_version_supported(schema_version: u64) -> bool {
    (SUPPORTED_BD_SCHEMA_MIN..=SUPPORTED_BD_SCHEMA_MAX).contains(&schema_version)
}

/// Version information about the local `bd` binary.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BdVersionInfo {
    /// Whether a `bd` binary was found and ran successfully.
    pub available: bool,
    /// bd's own version string (e.g. "1.0.5"), when reported.
    pub version: Option<String>,
    /// The beads DB schema version the binary speaks, when reported
    /// (`bd version --json` emits it; older binaries do not).
    pub schema_version: Option<u64>,
}

impl BdVersionInfo {
    fn unavailable() -> Self {
        BdVersionInfo {
            available: false,
            version: None,
            schema_version: None,
        }
    }
}

/// Parse the stdout of `bd version [--json]` into a [`BdVersionInfo`].
///
/// Prefers the JSON shape (`{"version":"1.0.5","schema_version":1,...}`);
/// falls back to the plain-text shape (`bd version 1.0.5 (Homebrew)`) for
/// older binaries, in which case schema_version is unknown.
fn parse_bd_version_output(stdout: &str) -> BdVersionInfo {
    #[derive(Deserialize)]
    struct RawBdVersion {
        version: Option<String>,
        schema_version: Option<u64>,
    }

    if let Ok(raw) = serde_json::from_str::<RawBdVersion>(stdout) {
        return BdVersionInfo {
            available: true,
            version: raw.version,
            schema_version: raw.schema_version,
        };
    }

    // Plain-text fallback: the version is the token after "version".
    let version = stdout
        .split_whitespace()
        .skip_while(|t| *t != "version")
        .nth(1)
        .map(str::to_string);

    BdVersionInfo {
        available: true,
        version,
        schema_version: None,
    }
}

/// Run `bd version --json` and parse the result.
fn detect_bd_version() -> BdVersionInfo {
    use std::process::Command;

    match Command::new("bd").args(["version", "--json"]).output() {
        Ok(output) if output.status.success() => {
            parse_bd_version_output(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => {
            tracing::warn!(
                "bd version --json exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            BdVersionInfo::unavailable()
        }
        Err(e) => {
            tracing::warn!("failed to spawn bd for version detection: {}", e);
            BdVersionInfo::unavailable()
        }
    }
}

/// Version info for the local `bd` binary, cached after the first successful
/// detection (the binary does not change under a running server; failed
/// detections are retried so bd installed after startup is still picked up).
pub fn bd_version_info() -> BdVersionInfo {
    static CACHE: OnceLock<BdVersionInfo> = OnceLock::new();

    if let Some(info) = CACHE.get() {
        return info.clone();
    }
    let info = detect_bd_version();
    if info.available {
        let _ = CACHE.set(info.clone());
    }
    info
}

/// Inclusive range of bd schema versions this server supports.
#[derive(Debug, Serialize)]
pub struct SupportedBdSchema {
    pub min: u64,
    pub max: u64,
}

/// Response body for `GET /api/version`.
#[derive(Debug, Serialize)]
pub struct VersionResponse {
    /// The UI↔server contract version this server speaks.
    pub api_version: u32,
    /// This server crate's version.
    pub server_version: &'static str,
    /// Observed local `bd` binary version info.
    pub bd: BdVersionInfo,
    /// bd schema versions this server is known to work against.
    pub supported_bd_schema: SupportedBdSchema,
    /// Whether bd's schema_version is in the supported range.
    /// `null` when bd (or its schema_version) is unknown.
    pub bd_schema_supported: Option<bool>,
}

/// GET /api/version
///
/// Reports the API contract version, the server build version, and the
/// observed bd binary/schema version so the UI (or an operator) can detect
/// version skew explicitly.
pub async fn api_version() -> impl IntoResponse {
    let bd = bd_version_info();
    let bd_schema_supported = bd.schema_version.map(schema_version_supported);

    Json(VersionResponse {
        api_version: API_CONTRACT_VERSION,
        server_version: env!("CARGO_PKG_VERSION"),
        bd,
        supported_bd_schema: SupportedBdSchema {
            min: SUPPORTED_BD_SCHEMA_MIN,
            max: SUPPORTED_BD_SCHEMA_MAX,
        },
        bd_schema_supported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    /// Golden fixture: verbatim `bd version --json` output (bd 1.0.5).
    const BD_VERSION_JSON: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/bd_version.json"
    ));

    #[test]
    fn parses_bd_version_json_fixture() {
        let info = parse_bd_version_output(BD_VERSION_JSON);
        assert!(info.available);
        assert_eq!(info.version.as_deref(), Some("1.0.5"));
        assert_eq!(info.schema_version, Some(1));
    }

    #[test]
    fn parses_plain_text_bd_version_fallback() {
        // Older bd without --json prints a plain line; schema stays unknown.
        let info = parse_bd_version_output("bd version 1.0.5 (Homebrew)\n");
        assert!(info.available);
        assert_eq!(info.version.as_deref(), Some("1.0.5"));
        assert_eq!(info.schema_version, None);
    }

    #[test]
    fn parses_unrecognized_output_without_panicking() {
        let info = parse_bd_version_output("something unexpected entirely");
        assert!(info.available);
        assert_eq!(info.version, None);
        assert_eq!(info.schema_version, None);
    }

    #[test]
    fn schema_version_supported_bounds() {
        assert!(schema_version_supported(SUPPORTED_BD_SCHEMA_MIN));
        assert!(schema_version_supported(SUPPORTED_BD_SCHEMA_MAX));
        assert!(!schema_version_supported(SUPPORTED_BD_SCHEMA_MAX + 1));
        if SUPPORTED_BD_SCHEMA_MIN > 0 {
            assert!(!schema_version_supported(SUPPORTED_BD_SCHEMA_MIN - 1));
        }
    }

    #[test]
    fn api_version_header_constants_are_valid() {
        // HeaderName::from_static panics on invalid/uppercase names, and the
        // version must serialize to a valid header value — both are used at
        // server startup, so catch regressions here instead of at boot.
        let name = HeaderName::from_static(API_VERSION_HEADER);
        assert_eq!(name.as_str(), API_VERSION_HEADER);
        assert!(HeaderValue::from_str(&API_CONTRACT_VERSION.to_string()).is_ok());
    }

    #[test]
    fn version_response_serializes_expected_shape() {
        let response = VersionResponse {
            api_version: API_CONTRACT_VERSION,
            server_version: "0.1.0",
            bd: BdVersionInfo {
                available: true,
                version: Some("1.0.5".to_string()),
                schema_version: Some(1),
            },
            supported_bd_schema: SupportedBdSchema {
                min: SUPPORTED_BD_SCHEMA_MIN,
                max: SUPPORTED_BD_SCHEMA_MAX,
            },
            bd_schema_supported: Some(true),
        };
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["api_version"], API_CONTRACT_VERSION);
        assert_eq!(json["server_version"], "0.1.0");
        assert_eq!(json["bd"]["available"], true);
        assert_eq!(json["bd"]["version"], "1.0.5");
        assert_eq!(json["bd"]["schema_version"], 1);
        assert_eq!(json["supported_bd_schema"]["min"], SUPPORTED_BD_SCHEMA_MIN);
        assert_eq!(json["supported_bd_schema"]["max"], SUPPORTED_BD_SCHEMA_MAX);
        assert_eq!(json["bd_schema_supported"], true);
    }

    #[test]
    fn version_response_unknown_schema_serializes_null() {
        let response = VersionResponse {
            api_version: API_CONTRACT_VERSION,
            server_version: "0.1.0",
            bd: BdVersionInfo::unavailable(),
            supported_bd_schema: SupportedBdSchema {
                min: SUPPORTED_BD_SCHEMA_MIN,
                max: SUPPORTED_BD_SCHEMA_MAX,
            },
            bd_schema_supported: None,
        };
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["bd"]["available"], false);
        assert!(json["bd"]["version"].is_null());
        assert!(json["bd"]["schema_version"].is_null());
        assert!(json["bd_schema_supported"].is_null());
    }
}
