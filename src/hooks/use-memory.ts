"use client";

/**
 * Hook for loading and managing memory entries (knowledge base).
 *
 * Fetches from GET /api/memory with server-side search + facet filters
 * (q, type, source, tag, since/until) and provides add, edit, archive, and
 * delete capabilities. Client-side filtering is kept as a fallback for
 * older servers that ignore the filter params.
 */

import { useState, useEffect, useCallback, useRef, useMemo } from "react";

import * as api from "@/lib/api";
import type {
  MemoryEntry,
  MemoryFacets,
  MemoryListFilters,
  MemoryStats,
  MemoryType,
} from "@/types";

export interface UseMemoryResult {
  /** Memory entries matching the active filters (server-filtered) */
  entries: MemoryEntry[];
  /** Aggregate stats over the full (unfiltered) knowledge base */
  stats: MemoryStats | null;
  /** Facet value counts (type/source/tag) over the full knowledge base */
  facets: MemoryFacets | null;
  /** Whether entries are currently being loaded */
  isLoading: boolean;
  /** Any error that occurred during loading */
  error: Error | null;
  /** Current search query */
  search: string;
  /** Set search query */
  setSearch: (value: string) => void;
  /** Current type filter (null = all) */
  typeFilter: MemoryType | null;
  /** Set type filter */
  setTypeFilter: (value: MemoryType | null) => void;
  /** Current source filter (null = all) */
  sourceFilter: string | null;
  /** Set source filter */
  setSourceFilter: (value: string | null) => void;
  /** Current tag filter (null = all) */
  tagFilter: string | null;
  /** Set tag filter */
  setTagFilter: (value: string | null) => void;
  /** Inclusive lower date bound (YYYY-MM-DD or unix seconds, null = none) */
  since: string | null;
  /** Set lower date bound */
  setSince: (value: string | null) => void;
  /** Inclusive upper date bound (YYYY-MM-DD or unix seconds, null = none) */
  until: string | null;
  /** Set upper date bound */
  setUntil: (value: string | null) => void;
  /** Entries after the client-side fallback filter (mirrors server filters) */
  filteredEntries: MemoryEntry[];
  /** Create a new entry (optimistic insert, rolled back on failure) */
  addEntry: (
    content: string,
    type: MemoryType,
    tags: string[],
    bead?: string
  ) => Promise<void>;
  /** Edit an entry's content and/or tags */
  editEntry: (key: string, content?: string, tags?: string[]) => Promise<void>;
  /** Archive an entry (move to archive file) */
  archiveEntry: (key: string) => Promise<void>;
  /** Permanently delete an entry */
  deleteEntry: (key: string) => Promise<void>;
  /** Manually refresh entries */
  refresh: () => Promise<void>;
}

const EMPTY_STATS: MemoryStats = {
  total: 0,
  learned: 0,
  investigation: 0,
  archived: 0,
  by_type: {},
};

/** Debounce delay for the search input before re-querying the server */
const SEARCH_DEBOUNCE_MS = 300;

/**
 * Parse a date-range bound the same way the server does: unix epoch seconds
 * or a YYYY-MM-DD calendar date in UTC (end of day for upper bounds).
 * Returns null when the value is blank or unparseable.
 */
function parseDateBound(value: string, endOfDay: boolean): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (/^-?\d+$/.test(trimmed)) return parseInt(trimmed, 10);

  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(trimmed);
  if (!match) return null;

  const [, year, month, day] = match;
  const ms = endOfDay
    ? Date.UTC(+year, +month - 1, +day, 23, 59, 59)
    : Date.UTC(+year, +month - 1, +day);
  return Math.floor(ms / 1000);
}

/**
 * Adjust stats by delta for a given entry type (used for optimistic updates).
 */
function shiftStats(
  stats: MemoryStats | null,
  type: MemoryType,
  delta: number
): MemoryStats {
  const base = stats ?? EMPTY_STATS;
  const by_type = { ...base.by_type };
  const next = (by_type[type] ?? 0) + delta;
  if (next > 0) by_type[type] = next;
  else delete by_type[type];
  return {
    ...base,
    total: base.total + delta,
    learned: base.learned + (type === "learned" ? delta : 0),
    investigation: base.investigation + (type === "investigation" ? delta : 0),
    by_type,
  };
}

/**
 * Hook to load and manage memory entries from a project's knowledge base.
 *
 * @param projectPath - The absolute path to the project root
 * @returns Object containing entries, stats, facets, filters, mutations, and refresh
 */
export function useMemory(projectPath: string): UseMemoryResult {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [stats, setStats] = useState<MemoryStats | null>(null);
  const [facets, setFacets] = useState<MemoryFacets | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState<MemoryType | null>(null);
  const [sourceFilter, setSourceFilter] = useState<string | null>(null);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [since, setSince] = useState<string | null>(null);
  const [until, setUntil] = useState<string | null>(null);

  // Debounced search value used for server queries
  const [debouncedSearch, setDebouncedSearch] = useState("");

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(search);
    }, SEARCH_DEBOUNCE_MS);

    return () => clearTimeout(timer);
  }, [search]);

  // Track if initial load has completed
  const hasLoadedRef = useRef(false);

  /**
   * Load memory entries from the API with the active server-side filters
   */
  const loadMemory = useCallback(async () => {
    if (!projectPath) {
      setEntries([]);
      setStats(EMPTY_STATS);
      setFacets(null);
      setIsLoading(false);
      return;
    }

    // Only show loading on initial load
    if (!hasLoadedRef.current) {
      setIsLoading(true);
    }

    const filters: MemoryListFilters = {
      q: debouncedSearch.trim() || undefined,
      type: typeFilter ?? undefined,
      source: sourceFilter ?? undefined,
      tag: tagFilter ?? undefined,
      since: since ?? undefined,
      until: until ?? undefined,
    };

    try {
      const response = await api.memory.list(projectPath, filters);
      setEntries(response.entries);
      setStats(response.stats);
      setFacets(response.facets ?? null);
      setError(null);
      hasLoadedRef.current = true;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      console.error("Failed to load memory:", error);
    } finally {
      setIsLoading(false);
    }
  }, [projectPath, debouncedSearch, typeFilter, sourceFilter, tagFilter, since, until]);

  /**
   * Public refresh function
   */
  const refresh = useCallback(async () => {
    await loadMemory();
  }, [loadMemory]);

  // Show the loading state again when switching projects
  useEffect(() => {
    hasLoadedRef.current = false;
  }, [projectPath]);

  // Load on mount and whenever the project or any filter changes
  useEffect(() => {
    loadMemory();
  }, [loadMemory]);

  /**
   * Client-side fallback filter mirroring the server-side filters.
   * A no-op when the server already applied them; keeps filtering
   * working against older servers that ignore the params.
   */
  const filteredEntries = useMemo(() => {
    let result = entries;

    // Apply type filter
    if (typeFilter) {
      result = result.filter((e) => e.type === typeFilter);
    }

    // Apply source filter (case-insensitive exact match)
    if (sourceFilter) {
      const source = sourceFilter.toLowerCase();
      result = result.filter((e) => e.source.toLowerCase() === source);
    }

    // Apply tag filter (case-insensitive exact match)
    if (tagFilter) {
      const tag = tagFilter.toLowerCase();
      result = result.filter((e) =>
        e.tags.some((t) => t.toLowerCase() === tag)
      );
    }

    // Apply date range (inclusive, same semantics as the server)
    const sinceTs = since ? parseDateBound(since, false) : null;
    if (sinceTs !== null) {
      result = result.filter((e) => e.ts >= sinceTs);
    }
    const untilTs = until ? parseDateBound(until, true) : null;
    if (untilTs !== null) {
      result = result.filter((e) => e.ts <= untilTs);
    }

    // Apply search filter
    if (search.trim()) {
      const query = search.toLowerCase();
      result = result.filter(
        (e) =>
          e.content.toLowerCase().includes(query) ||
          e.key.toLowerCase().includes(query) ||
          e.source.toLowerCase().includes(query) ||
          e.tags.some((t) => t.toLowerCase().includes(query)) ||
          e.bead.toLowerCase().includes(query)
      );
    }

    return result;
  }, [entries, search, typeFilter, sourceFilter, tagFilter, since, until]);

  /**
   * Create a new entry via POST /api/memory.
   *
   * Optimistically inserts the entry at the top of the list, then replaces
   * it with the server-authoritative entry (key/ts assigned server-side) on
   * success. On failure the optimistic entry is rolled back and the error
   * is rethrown for the caller to surface.
   */
  const addEntry = useCallback(
    async (content: string, type: MemoryType, tags: string[], bead?: string) => {
      if (!projectPath) return;

      const optimistic: MemoryEntry = {
        key: `pending-${Date.now()}`,
        type,
        content,
        source: "sweetgrass-ui",
        tags,
        ts: Math.floor(Date.now() / 1000),
        bead: bead ?? "",
      };

      // Optimistic insert (newest first)
      setEntries((prev) => [optimistic, ...prev]);
      setStats((prev) => shiftStats(prev, type, 1));

      try {
        const response = await api.memory.create(
          projectPath,
          content,
          type,
          tags,
          bead
        );
        // Swap in the server-authoritative entry
        setEntries((prev) =>
          prev.map((e) => (e.key === optimistic.key ? response.entry : e))
        );
      } catch (err) {
        // Roll back the optimistic insert
        setEntries((prev) => prev.filter((e) => e.key !== optimistic.key));
        setStats((prev) => shiftStats(prev, type, -1));
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to add memory entry:", error);
        throw error;
      }
    },
    [projectPath]
  );

  /**
   * Edit an entry's content and/or tags
   */
  const editEntry = useCallback(
    async (key: string, content?: string, tags?: string[]) => {
      if (!projectPath) return;
      try {
        await api.memory.update(projectPath, key, content, tags);
        await loadMemory();
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to edit memory entry:", error);
        throw error;
      }
    },
    [projectPath, loadMemory]
  );

  /**
   * Archive an entry
   */
  const archiveEntry = useCallback(
    async (key: string) => {
      if (!projectPath) return;
      try {
        await api.memory.remove(projectPath, key, true);
        await loadMemory();
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to archive memory entry:", error);
        throw error;
      }
    },
    [projectPath, loadMemory]
  );

  /**
   * Permanently delete an entry
   */
  const deleteEntry = useCallback(
    async (key: string) => {
      if (!projectPath) return;
      try {
        await api.memory.remove(projectPath, key, false);
        await loadMemory();
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to delete memory entry:", error);
        throw error;
      }
    },
    [projectPath, loadMemory]
  );

  return {
    entries,
    stats,
    facets,
    isLoading,
    error,
    search,
    setSearch,
    typeFilter,
    setTypeFilter,
    sourceFilter,
    setSourceFilter,
    tagFilter,
    setTagFilter,
    since,
    setSince,
    until,
    setUntil,
    filteredEntries,
    addEntry,
    editEntry,
    archiveEntry,
    deleteEntry,
    refresh,
  };
}
