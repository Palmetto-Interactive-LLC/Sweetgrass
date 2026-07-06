/**
 * Epic parser utility for beads kanban
 *
 * Provides functions to separate epics from tasks, build epic trees,
 * compute progress metrics, and identify blocking relationships.
 */

import type { Bead, BeadStatus, Epic, EpicProgress } from "@/types";

/**
 * Separates epics from standalone tasks
 *
 * @param beads - Array of all beads
 * @returns Object with separate arrays for epics and standalone tasks
 *
 * @example
 * ```typescript
 * const { epics, tasks } = parseEpicsAndTasks(allBeads);
 * console.log(`Found ${epics.length} epics and ${tasks.length} standalone tasks`);
 * ```
 */
export function parseEpicsAndTasks(beads: Bead[]): {
  epics: Epic[];
  tasks: Bead[];
} {
  if (!beads || beads.length === 0) {
    return { epics: [], tasks: [] };
  }

  const epics: Epic[] = [];
  const tasks: Bead[] = [];

  for (const bead of beads) {
    // Bead is an epic if issue_type is 'epic' OR if it has children
    if (bead.issue_type === 'epic' || (bead.children && bead.children.length > 0)) {
      epics.push({
        ...bead,
        issue_type: 'epic',
        children: bead.children ?? [],
      } as Epic);
    } else if (!bead.parent_id) {
      // Only include tasks that are NOT children of epics (standalone tasks)
      tasks.push(bead);
    }
  }

  return { epics, tasks };
}

/**
 * Attaches child beads to their parent epics
 *
 * @param epics - Array of epic beads
 * @param allBeads - Array of all beads (including children)
 * @returns Array of epics with resolved children attached
 *
 * @example
 * ```typescript
 * const epicsWithChildren = buildEpicTree(epics, allBeads);
 * epicsWithChildren.forEach(epic => {
 *   console.log(`Epic ${epic.id} has ${epic.children.length} children`);
 * });
 * ```
 */
export function buildEpicTree(epics: Epic[], allBeads: Bead[]): Epic[] {
  if (!epics || epics.length === 0) {
    return [];
  }

  if (!allBeads || allBeads.length === 0) {
    return epics;
  }

  // Create a lookup map for fast child access
  const beadMap = new Map<string, Bead>();
  for (const bead of allBeads) {
    beadMap.set(bead.id, bead);
  }

  // Build epic tree with resolved children
  return epics.map((epic) => {
    const children = (epic.children ?? [])
      .map((childId) => beadMap.get(childId))
      .filter((child): child is Bead => child !== undefined);

    return {
      ...epic,
      children: children.map((c) => c.id),
    };
  });
}

/**
 * Computes progress metrics for an epic based on its children
 *
 * @param epic - Epic bead with children
 * @param allBeads - Array of all beads to resolve children from
 * @returns EpicProgress object with computed metrics
 *
 * @example
 * ```typescript
 * const progress = computeEpicProgress(epic, allBeads);
 * console.log(`${progress.completed}/${progress.total} children completed`);
 * console.log(`${progress.blocked} children blocked`);
 * ```
 */
export function computeEpicProgress(epic: Epic, allBeads: Bead[]): EpicProgress {
  if (!epic.children || epic.children.length === 0) {
    return {
      total: 0,
      completed: 0,
      inProgress: 0,
      blocked: 0,
    };
  }

  if (!allBeads || allBeads.length === 0) {
    return {
      total: epic.children.length,
      completed: 0,
      inProgress: 0,
      blocked: 0,
    };
  }

  // Create lookup map for children
  const beadMap = new Map<string, Bead>();
  for (const bead of allBeads) {
    beadMap.set(bead.id, bead);
  }

  // Resolve child beads
  const children = epic.children
    .map((childId) => beadMap.get(childId))
    .filter((child): child is Bead => child !== undefined);

  // Count statuses
  const completed = children.filter((c) => c.status === 'closed').length;
  const inProgress = children.filter((c) => c.status === 'in_progress').length;

  // Count blocked children (those with unresolved deps)
  const blocked = children.filter((child) => {
    if (!child.deps || child.deps.length === 0) {
      return false;
    }

    // Check if any dependency is not yet completed
    return child.deps.some((depId) => {
      const depBead = beadMap.get(depId);
      return depBead && depBead.status !== 'closed';
    });
  }).length;

  return {
    total: children.length,
    completed,
    inProgress,
    blocked,
  };
}

/**
 * Derives an epic's effective status (kanban column) from its children.
 *
 * An epic's status should follow its sub-issues rather than its own stored
 * status, which the beads CLI does not auto-advance. Rules, in priority order:
 * - Any child in progress  -> epic is in_progress
 * - Else any child in review -> epic is inreview
 * - Else children exist and all are closed -> epic is closed
 * - Else (no children, or only open children) -> fall back to the epic's own
 *   status (so an explicitly closed/in-review epic with no children is respected)
 *
 * @param epic - The epic bead
 * @param allBeads - All beads, to resolve the epic's children
 * @returns The BeadStatus the epic should be grouped under
 */
export function deriveEpicStatus(epic: Epic, allBeads: Bead[]): BeadStatus {
  const childIds = epic.children ?? [];
  if (childIds.length === 0) return epic.status;

  const beadMap = new Map<string, Bead>();
  for (const bead of allBeads) beadMap.set(bead.id, bead);

  const children = childIds
    .map((id) => beadMap.get(id))
    .filter((c): c is Bead => c !== undefined);

  if (children.length === 0) return epic.status;

  if (children.some((c) => c.status === 'in_progress')) return 'in_progress';
  if (children.some((c) => c.status === 'inreview')) return 'inreview';
  if (children.every((c) => c.status === 'closed')) return 'closed';

  // Only open (and/or a mix that isn't yet active) children remain.
  return 'open';
}

/**
 * Identifies tasks that are blocked by unresolved dependencies
 *
 * @param beads - Array of all beads to check
 * @returns Array of beads that have blocking dependencies
 *
 * @example
 * ```typescript
 * const blockedTasks = getBlockedTasks(allBeads);
 * console.log(`${blockedTasks.length} tasks are currently blocked`);
 * blockedTasks.forEach(task => {
 *   console.log(`${task.id} blocked by: ${task.deps?.join(', ')}`);
 * });
 * ```
 */
export function getBlockedTasks(beads: Bead[]): Bead[] {
  if (!beads || beads.length === 0) {
    return [];
  }

  // Create lookup map for fast access
  const beadMap = new Map<string, Bead>();
  for (const bead of beads) {
    beadMap.set(bead.id, bead);
  }

  // Filter beads with unresolved dependencies
  return beads.filter((bead) => {
    if (!bead.deps || bead.deps.length === 0) {
      return false;
    }

    // Check if any dependency is not yet completed
    return bead.deps.some((depId) => {
      const depBead = beadMap.get(depId);
      // Blocked if dependency exists and is not closed
      return depBead && depBead.status !== 'closed';
    });
  });
}

/**
 * Computes which beads the given bead blocks (inverse of deps)
 *
 * @param bead - The bead to check
 * @param allBeads - Array of all beads to search for dependents
 * @returns Array of bead IDs that depend on this bead
 *
 * @example
 * ```typescript
 * const blockers = computeBlockers(someBead, allBeads);
 * if (blockers.length > 0) {
 *   console.log(`Completing this task will unblock: ${blockers.join(', ')}`);
 * }
 * ```
 */
export function computeBlockers(bead: Bead, allBeads: Bead[]): string[] {
  if (!bead || !bead.id) {
    return [];
  }

  if (!allBeads || allBeads.length === 0) {
    return [];
  }

  // Find all beads that list this bead in their deps array
  const blockers: string[] = [];

  for (const otherBead of allBeads) {
    if (!otherBead.deps || otherBead.deps.length === 0) {
      continue;
    }

    // If this bead is in the other bead's deps, then this bead blocks it
    if (otherBead.deps.includes(bead.id)) {
      blockers.push(otherBead.id);
    }
  }

  return blockers;
}
