"use client";

import { useState, useCallback } from "react";

import {
  Database,
  Plus,
  Search,
  Trash2,
  X,
  Loader2,
} from "lucide-react";

import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogClose,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { SheetFooter } from "@/components/ui/sheet";
import { useBdMemories } from "@/hooks/use-bd-memories";
import type { KvMemory } from "@/types";

export interface BdMemoryTabProps {
  /** Absolute path to the project root (bd cwd) */
  projectPath: string;
}

/**
 * Single KV memory card: key (mono) + value + delete action.
 */
function KvMemoryCard({
  memory,
  onDelete,
}: {
  memory: KvMemory;
  onDelete: (key: string) => void;
}) {
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-3 space-y-1.5 overflow-hidden">
      <div className="flex items-center justify-between gap-2">
        <span
          className="text-xs font-mono text-emerald-400/80 truncate"
          title={memory.key}
        >
          {memory.key}
        </span>
        <button
          type="button"
          onClick={() => onDelete(memory.key)}
          className="size-6 shrink-0 flex items-center justify-center rounded text-zinc-500 hover:text-red-400 hover:bg-zinc-800 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          aria-label={`Forget memory ${memory.key}`}
        >
          <Trash2 className="size-3.5" />
        </button>
      </div>
      <p className="text-sm text-zinc-300 whitespace-pre-wrap break-words text-pretty">
        {memory.value}
      </p>
    </div>
  );
}

/**
 * CLI Memory tab — lists the upstream bd KV memories (System A).
 *
 * These come from `bd memories --json`: key→value strings stored in the
 * embedded beads database and injected into agent sessions via `bd prime`.
 * This is a separate system from the knowledge.jsonl entries shown in the
 * Knowledge tab, and the UI keeps the two clearly apart.
 */
export function BdMemoryTab({ projectPath }: BdMemoryTabProps) {
  const {
    memories,
    isLoading,
    error,
    search,
    setSearch,
    filteredMemories,
    addMemory,
    forgetMemory,
  } = useBdMemories(projectPath);

  // Add form state
  const [isAdding, setIsAdding] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  // Delete confirmation state
  const [deletingKey, setDeletingKey] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  /**
   * Save a new memory via `bd remember`
   */
  const handleAddSave = useCallback(async () => {
    const value = newValue.trim();
    if (!value) return;
    setIsSaving(true);
    try {
      await addMemory(value, newKey.trim() || undefined);
      setNewKey("");
      setNewValue("");
      setIsAdding(false);
    } catch {
      // Error is logged in hook
    } finally {
      setIsSaving(false);
    }
  }, [newKey, newValue, addMemory]);

  /**
   * Confirm delete via `bd forget`
   */
  const handleDeleteConfirm = useCallback(async () => {
    if (!deletingKey) return;
    setIsDeleting(true);
    try {
      await forgetMemory(deletingKey);
      setDeletingKey(null);
    } catch {
      // Error is logged in hook
    } finally {
      setIsDeleting(false);
    }
  }, [deletingKey, forgetMemory]);

  return (
    <>
      {/* System A explainer — keeps the two memory systems clearly apart */}
      <p className="mt-3 rounded-md border border-zinc-800 bg-zinc-900/50 px-3 py-2 text-xs text-zinc-500 text-pretty">
        Built-in beads CLI memory: key→value notes stored in the beads
        database via{" "}
        <code className="font-mono text-zinc-400">bd remember</code> and
        injected into every agent session by{" "}
        <code className="font-mono text-zinc-400">bd prime</code>. Separate
        from the knowledge.jsonl entries in the Knowledge tab.
      </p>

      {/* Search + add row */}
      <div className="flex items-center gap-2 mt-3">
        <div className="relative flex-1">
          <Search
            className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-zinc-500"
            aria-hidden="true"
          />
          <Input
            type="text"
            aria-label="Search bd memories"
            placeholder="Search bd memories..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="pl-8 pr-8 h-8 bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500"
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              className="absolute right-0 top-1/2 -translate-y-1/2 size-8 flex items-center justify-center text-zinc-500 hover:text-zinc-300"
              aria-label="Clear search"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <Button
          size="sm"
          variant={isAdding ? "secondary" : "primary"}
          className="h-8 shrink-0"
          onClick={() => setIsAdding((v) => !v)}
          aria-expanded={isAdding}
        >
          <Plus className="size-3.5 mr-1" aria-hidden="true" />
          Add
        </Button>
      </div>

      {/* Add form */}
      {isAdding && (
        <div className="mt-3 rounded-lg border border-zinc-800 bg-zinc-900/50 p-3 space-y-2">
          <Input
            aria-label="Memory key (optional)"
            placeholder="Key (optional — generated from content)"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            className="h-8 bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500 font-mono text-xs"
          />
          <textarea
            aria-label="Memory content"
            placeholder="What should agents remember?"
            value={newValue}
            onChange={(e) => setNewValue(e.target.value)}
            className="w-full h-20 rounded-md border border-zinc-700 bg-zinc-800/50 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
          />
          <div className="flex justify-end gap-2">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setIsAdding(false)}
            >
              Cancel
            </Button>
            <Button
              size="sm"
              onClick={handleAddSave}
              disabled={isSaving || !newValue.trim()}
            >
              {isSaving ? (
                <Loader2
                  className="size-3.5 animate-spin mr-1"
                  aria-hidden="true"
                />
              ) : null}
              Remember
            </Button>
          </div>
        </div>
      )}

      {/* Memories list */}
      <ScrollArea className="flex-1 mt-3 -mx-6 px-6">
        <div className="space-y-2 pb-4">
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2
                className="size-5 text-zinc-500 animate-spin"
                aria-hidden="true"
              />
              <span className="sr-only">Loading bd memories</span>
            </div>
          ) : error ? (
            <div
              role="alert"
              className="rounded-lg border border-red-500/30 bg-red-500/10 p-4 text-center"
            >
              <p className="text-sm text-red-400">
                Failed to load bd memories
              </p>
              <p className="text-xs text-red-400/60 mt-1">{error.message}</p>
              <p className="text-xs text-red-400/60 mt-1">
                Requires the bd CLI and a beads database in this project.
              </p>
            </div>
          ) : filteredMemories.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center">
              <Database
                className="size-8 text-zinc-700 mb-3"
                aria-hidden="true"
              />
              <p className="text-sm text-zinc-500">
                {search
                  ? "No memories match your search"
                  : "No bd memories yet"}
              </p>
              {search ? (
                <button
                  type="button"
                  onClick={() => setSearch("")}
                  className="mt-2 text-xs text-zinc-500 hover:text-zinc-300 underline underline-offset-2 rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                >
                  Clear search
                </button>
              ) : (
                <p className="text-xs text-zinc-600 mt-1">
                  Add one here or run{" "}
                  <code className="font-mono">bd remember</code> in the
                  project.
                </p>
              )}
            </div>
          ) : (
            filteredMemories.map((memory) => (
              <KvMemoryCard
                key={memory.key}
                memory={memory}
                onDelete={setDeletingKey}
              />
            ))
          )}
        </div>
      </ScrollArea>

      {/* Footer stats */}
      {!isLoading && !error && memories.length > 0 && (
        <SheetFooter className="border-t border-zinc-800 pt-3 -mx-6 px-6">
          <p className="text-xs text-zinc-600 w-full text-center">
            <span className="tabular-nums">{memories.length}</span>{" "}
            {memories.length === 1 ? "memory" : "memories"} in the beads
            database
          </p>
        </SheetFooter>
      )}

      {/* Delete Confirmation Dialog */}
      <AlertDialog
        open={!!deletingKey}
        onOpenChange={(isOpen) => !isOpen && setDeletingKey(null)}
      >
        <AlertDialogContent className="bg-zinc-900 border-zinc-800">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-zinc-100">
              Forget Memory
            </AlertDialogTitle>
            <AlertDialogDescription className="text-zinc-400">
              This runs{" "}
              <code className="font-mono text-zinc-300">
                bd forget {deletingKey}
              </code>{" "}
              and permanently removes the memory from the beads database. It
              will no longer be injected into agent sessions.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose render={<Button variant="ghost">Cancel</Button>} />
            <Button
              variant="destructive"
              onClick={handleDeleteConfirm}
              disabled={isDeleting}
            >
              {isDeleting ? (
                <Loader2
                  className="size-4 animate-spin mr-1.5"
                  aria-hidden="true"
                />
              ) : (
                <Trash2 className="size-4 mr-1.5" aria-hidden="true" />
              )}
              Forget
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
