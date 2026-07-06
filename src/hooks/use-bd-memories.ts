"use client";

/**
 * Hook for loading and managing upstream bd KV memories (System A).
 *
 * These are keyed key→string rows stored in the embedded beads database
 * (`bd remember` / `bd memories` / `bd forget`) and injected into agent
 * sessions via `bd prime`. They are a DISTINCT system from the
 * `.beads/memory/knowledge.jsonl` entries managed by `useMemory`.
 *
 * Fetches via POST /api/bd/command (`bd memories --json`) and provides
 * search, add, and delete capabilities.
 */

import { useState, useEffect, useCallback, useRef, useMemo } from "react";

import * as api from "@/lib/api";
import type { KvMemory } from "@/types";

export interface UseBdMemoriesResult {
  /** All KV memories from the project's beads database */
  memories: KvMemory[];
  /** Whether memories are currently being loaded */
  isLoading: boolean;
  /** Any error that occurred during loading */
  error: Error | null;
  /** Current search query */
  search: string;
  /** Set search query */
  setSearch: (value: string) => void;
  /** Memories filtered by search */
  filteredMemories: KvMemory[];
  /** Store a new memory (or update in place when `key` already exists) */
  addMemory: (value: string, key?: string) => Promise<void>;
  /** Delete a memory by key */
  forgetMemory: (key: string) => Promise<void>;
  /** Manually refresh memories */
  refresh: () => Promise<void>;
}

/**
 * Hook to load and manage upstream bd KV memories for a project.
 *
 * @param projectPath - The absolute path to the project root (bd cwd)
 * @returns Object containing memories, search, mutations, and refresh
 */
export function useBdMemories(projectPath: string): UseBdMemoriesResult {
  const [memories, setMemories] = useState<KvMemory[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [search, setSearch] = useState("");

  // Track if initial load has completed
  const hasLoadedRef = useRef(false);

  /**
   * Load KV memories via `bd memories --json`
   */
  const loadMemories = useCallback(async () => {
    if (!projectPath) {
      setMemories([]);
      setIsLoading(false);
      return;
    }

    // Only show loading on initial load
    if (!hasLoadedRef.current) {
      setIsLoading(true);
    }

    try {
      const result = await api.bd.listMemories(projectPath);
      setMemories(result);
      setError(null);
      hasLoadedRef.current = true;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      console.error("Failed to load bd memories:", error);
    } finally {
      setIsLoading(false);
    }
  }, [projectPath]);

  /**
   * Public refresh function
   */
  const refresh = useCallback(async () => {
    await loadMemories();
  }, [loadMemories]);

  // Initial load when project path changes
  useEffect(() => {
    hasLoadedRef.current = false;
    loadMemories();
  }, [loadMemories]);

  /**
   * Filter memories by search query (matches key or value)
   */
  const filteredMemories = useMemo(() => {
    if (!search.trim()) return memories;
    const query = search.toLowerCase();
    return memories.filter(
      (m) =>
        m.key.toLowerCase().includes(query) ||
        m.value.toLowerCase().includes(query)
    );
  }, [memories, search]);

  /**
   * Store a new memory via `bd remember`
   */
  const addMemory = useCallback(
    async (value: string, key?: string) => {
      if (!projectPath) return;
      try {
        await api.bd.remember(projectPath, value, key);
        await loadMemories();
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to add bd memory:", error);
        throw error;
      }
    },
    [projectPath, loadMemories]
  );

  /**
   * Delete a memory via `bd forget`
   */
  const forgetMemory = useCallback(
    async (key: string) => {
      if (!projectPath) return;
      try {
        await api.bd.forget(projectPath, key);
        await loadMemories();
      } catch (err) {
        const error = err instanceof Error ? err : new Error(String(err));
        console.error("Failed to forget bd memory:", error);
        throw error;
      }
    },
    [projectPath, loadMemories]
  );

  return {
    memories,
    isLoading,
    error,
    search,
    setSearch,
    filteredMemories,
    addMemory,
    forgetMemory,
    refresh,
  };
}
