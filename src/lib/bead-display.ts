/**
 * Shared display helpers for beads — status colors, labels, IDs, blocked detection.
 *
 * Consolidates logic that was previously duplicated across bead-card, bead-detail,
 * epic-card, and subtask-list so the epic-detail and table views render consistently.
 */

import type { Bead, BeadStatus, StatusBadgeInfo } from "@/types";

/** Human-readable label for a status column. */
export function statusLabel(status: BeadStatus): string {
  switch (status) {
    case "open":
      return "Open";
    case "in_progress":
      return "In Progress";
    case "inreview":
      return "In Review";
    case "closed":
      return "Closed";
    default:
      return "Open";
  }
}

/** Text color class per status (matches subtask-list / bead-detail convention). */
export function statusTextColor(status: BeadStatus): string {
  switch (status) {
    case "closed":
      return "text-green-400";
    case "in_progress":
      return "text-blue-400";
    case "inreview":
      return "text-purple-400";
    case "open":
    default:
      return "text-zinc-400";
  }
}

/** Dot fill color class per status (for the small circle indicator). */
export function statusDotColor(status: BeadStatus): string {
  switch (status) {
    case "closed":
      return "text-green-500";
    case "in_progress":
      return "text-blue-500";
    case "inreview":
      return "text-purple-500";
    case "open":
    default:
      return "text-zinc-500";
  }
}

/** Priority label (P0–P4). */
export function priorityLabel(priority: number): string {
  return `P${priority}`;
}

/** Priority color class. P0 = red (urgent) → P4 = zinc (low). */
export function priorityColor(priority: number): string {
  switch (priority) {
    case 0:
      return "text-red-400";
    case 1:
      return "text-orange-400";
    case 2:
      return "text-zinc-400";
    case 3:
    case 4:
    default:
      return "text-zinc-500";
  }
}

/** Badge classes for a status-badge variant (warning/muted/info). */
export function statusBadgeClasses(variant: StatusBadgeInfo["variant"]): string {
  switch (variant) {
    case "warning":
      return "bg-orange-500/15 text-orange-400 border-orange-600/30";
    case "muted":
      return "bg-zinc-500/15 text-zinc-400 border-zinc-600/30";
    case "info":
      return "bg-blue-500/15 text-blue-400 border-blue-600/30";
    default:
      return "bg-zinc-500/15 text-zinc-400 border-zinc-600/30";
  }
}

/**
 * A bead is blocked when it is not closed and has unresolved dependencies.
 * The backend only includes UNRESOLVED deps in `deps`, so any entry means blocked.
 */
export function isBlocked(bead: Bead): boolean {
  if (bead.status === "closed") return false;
  return (bead.deps ?? []).length > 0;
}

/**
 * A bead is "ready to work" when it's open and NOT blocked — i.e. actionable now.
 * (deps only contains unresolved blockers, so empty deps = all blockers cleared.)
 */
export function isReadyToWork(bead: Bead): boolean {
  return bead.status === "open" && (bead.deps ?? []).length === 0;
}

/**
 * Short display form of a bead ID. Preserves hierarchical suffixes (e.g. oi-19s.9.4)
 * which the table/epic views rely on, unlike the card's aggressive truncation.
 */
export function formatBeadId(id: string): string {
  const dashIdx = id.indexOf("-");
  if (dashIdx === -1) return id;
  // Keep everything after the first dash (the meaningful hierarchical id part).
  return id.slice(dashIdx + 1);
}

/** Depth of a bead in the hierarchy, inferred from dotted ID (oi-19s.9.4 → 2). */
export function beadDepth(id: string): number {
  const afterDash = id.includes("-") ? id.slice(id.indexOf("-") + 1) : id;
  return (afterDash.match(/\./g) ?? []).length;
}
