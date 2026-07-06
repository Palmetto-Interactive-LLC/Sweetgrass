"use client";

/**
 * Global banner that surfaces UI↔server version skew.
 *
 * Renders nothing in the normal case. Shows a fixed warning banner when:
 * - the server's API contract version differs from the one this UI build
 *   speaks (detected via the `x-sweetgrass-api-version` response header or
 *   the /api/version endpoint), or
 * - the local bd binary's schema_version falls outside the range the server
 *   supports (a schema bump — fork-risk class, must not break silently).
 */

import { useEffect, useState } from "react";

import * as api from "@/lib/api";

type ContractProblem =
  | { kind: "api-version"; expected: number; actual: number }
  | { kind: "bd-schema"; schemaVersion: number; min: number; max: number };

export function ApiVersionBanner() {
  const [problem, setProblem] = useState<ContractProblem | null>(null);

  useEffect(() => {
    // Surface a mismatch recorded before this component mounted…
    const existing = api.getApiVersionMismatch();
    if (existing) {
      setProblem({ kind: "api-version", ...existing });
    }

    // …and any detected by later API calls.
    const onMismatch = (e: Event) => {
      const detail = (e as CustomEvent<api.ApiVersionMismatch>).detail;
      setProblem((prev) => prev ?? { kind: "api-version", ...detail });
    };
    window.addEventListener(api.API_VERSION_MISMATCH_EVENT, onMismatch);

    // Ask the server for its contract + bd schema versions once on mount.
    let cancelled = false;
    api.version
      .get()
      .then((info) => {
        if (cancelled) return;
        const schemaVersion = info.bd.schema_version;
        if (info.api_version !== api.EXPECTED_API_VERSION) {
          setProblem(
            (prev) =>
              prev ?? {
                kind: "api-version",
                expected: api.EXPECTED_API_VERSION,
                actual: info.api_version,
              }
          );
        } else if (info.bd_schema_supported === false && schemaVersion !== null) {
          setProblem(
            (prev) =>
              prev ?? {
                kind: "bd-schema",
                schemaVersion,
                min: info.supported_bd_schema.min,
                max: info.supported_bd_schema.max,
              }
          );
        }
      })
      .catch(() => {
        // Server unreachable or pre-versioning — nothing to report.
      });

    return () => {
      cancelled = true;
      window.removeEventListener(api.API_VERSION_MISMATCH_EVENT, onMismatch);
    };
  }, []);

  if (!problem) return null;

  const message =
    problem.kind === "api-version"
      ? `This UI speaks API contract v${problem.expected} but the server is on v${problem.actual}. ` +
        `Data may render incorrectly — redeploy so both sides match.`
      : `bd schema_version ${problem.schemaVersion} is outside this server's supported range ` +
        `(${problem.min}–${problem.max}). Bead data may be incomplete or wrong — update Sweetgrass or pin bd.`;

  return (
    <div
      role="alert"
      className="fixed inset-x-0 top-0 z-50 border-b border-amber-500/40 bg-amber-950/95 px-4 py-2 text-center text-sm text-amber-200"
    >
      <span className="font-semibold">Version skew detected.</span> {message}
    </div>
  );
}
