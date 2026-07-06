"use client";

import {
  useState,
  useCallback,
  useEffect,
  useMemo,
  type ComponentProps,
} from "react";

import {
  BrainCircuit,
  Bot,
  Pencil,
  Plus,
  Tag,
  ExternalLink,
  Archive,
  Trash2,
  Search,
  MoreVertical,
  SlidersHorizontal,
  X,
  Loader2,
} from "lucide-react";

import { BdMemoryTab } from "@/components/bd-memory-tab";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogClose,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
  SheetFooter,
} from "@/components/ui/sheet";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useMemory } from "@/hooks/use-memory";
import { cn } from "@/lib/utils";
import { CANONICAL_MEMORY_TYPES, KNOWN_MEMORY_TYPES } from "@/types";
import type { MemoryEntry, MemoryType } from "@/types";

export interface MemoryPanelProps {
  /** Whether the panel is open */
  open: boolean;
  /** Callback when open state changes */
  onOpenChange: (open: boolean) => void;
  /** Absolute path to the project root */
  projectPath: string;
  /** Callback to navigate to a bead by ID */
  onNavigateToBead?: (beadId: string) => void;
}

type TabFilter = "all" | MemoryType;

type BadgeVariant = ComponentProps<typeof Badge>["variant"];

/**
 * Badge label/color per known entry type. Unknown types fall back to a
 * neutral badge so new categories never break the panel.
 */
const TYPE_BADGES: Record<string, { label: string; variant: BadgeVariant }> = {
  learned: { label: "LEARN", variant: "info" },
  investigation: { label: "INVES", variant: "success" },
  decision: { label: "DECIS", variant: "warning" },
  gotcha: { label: "GOTCH", variant: "destructive" },
  convention: { label: "CONVN", variant: "primary" },
};

function typeBadge(type: string): { label: string; variant: BadgeVariant } {
  return (
    TYPE_BADGES[type] ?? {
      label: type.slice(0, 5).toUpperCase() || "OTHER",
      variant: "secondary",
    }
  );
}

/**
 * Capitalize an entry type for tab / footer display
 */
function formatTypeLabel(type: string): string {
  return type.charAt(0).toUpperCase() + type.slice(1);
}

const TAB_TRIGGER_CLASSES =
  "h-7 flex-1 text-sm font-medium data-[state=active]:bg-zinc-700 data-[state=active]:text-zinc-100 data-[state=inactive]:text-zinc-400";

/** Maximum number of tag chips shown per entry card */
const MAX_VISIBLE_TAGS = 3;

/**
 * Which memory system is displayed:
 * - "knowledge" — `.beads/memory/knowledge.jsonl` entries (System B)
 * - "kv" — upstream bd CLI key→value memories (System A)
 */
type MemorySystem = "knowledge" | "kv";

/**
 * Format a unix timestamp to a relative time string
 */
function formatRelativeTime(ts: number): string {
  const now = Date.now() / 1000;
  const diff = now - ts;

  if (diff < 60) return "just now";
  if (diff < 3600) {
    const mins = Math.floor(diff / 60);
    return `${mins}m ago`;
  }
  if (diff < 86400) {
    const hours = Math.floor(diff / 3600);
    return `${hours}h ago`;
  }
  if (diff < 604800) {
    const days = Math.floor(diff / 86400);
    return `${days}d ago`;
  }
  const date = new Date(ts * 1000);
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

/**
 * Format a unix timestamp to a full absolute date-time string (for tooltips)
 */
function formatFullTimestamp(ts: number): string {
  return new Date(ts * 1000).toLocaleString("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/**
 * Format a memory entry's bead ID to a short display form.
 * Handles IDs like "sweetgrass-t25.2" -> "BD-t25.2"
 * or "project-p02" -> "BD-p02"
 */
function formatMemoryBeadId(id: string): string {
  if (id.startsWith("BD-") || id.startsWith("bd-")) {
    return id.length > 10 ? `BD-${id.slice(-6)}` : id.toUpperCase();
  }
  // Extract the part after the last hyphen (handles dots for epic children)
  const lastDash = id.lastIndexOf("-");
  if (lastDash !== -1) {
    return `BD-${id.slice(lastDash + 1)}`;
  }
  return `BD-${id.slice(0, 6)}`;
}

/**
 * Single memory entry card
 */
function MemoryEntryCard({
  entry,
  activeTag,
  onTagClick,
  onEdit,
  onArchive,
  onDelete,
  onNavigate,
}: {
  entry: MemoryEntry;
  activeTag: string | null;
  onTagClick: (tag: string) => void;
  onEdit: (entry: MemoryEntry) => void;
  onArchive: (key: string) => void;
  onDelete: (key: string) => void;
  onNavigate?: (beadId: string) => void;
}) {
  const badge = typeBadge(entry.type);

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-3 space-y-2 overflow-hidden">
      {/* Top row: type badge, source badge, timestamp */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <Badge variant={badge.variant} appearance="light" size="xs">
            {badge.label}
          </Badge>
          {entry.source && (
            <Badge
              variant="secondary"
              appearance="outline"
              size="xs"
              className="text-zinc-400 min-w-0 max-w-[140px]"
              title={`Source: ${entry.source}`}
            >
              <Bot className="size-3 shrink-0" aria-hidden="true" />
              <span className="truncate">{entry.source}</span>
            </Badge>
          )}
        </div>
        <time
          dateTime={new Date(entry.ts * 1000).toISOString()}
          title={formatFullTimestamp(entry.ts)}
          className="text-xs text-zinc-600 shrink-0 tabular-nums"
          suppressHydrationWarning
        >
          {formatRelativeTime(entry.ts)}
        </time>
      </div>

      {/* Content preview */}
      <p className="text-sm text-zinc-300 line-clamp-3 text-pretty">
        {entry.content}
      </p>

      {/* Bottom row: tags, bead link, actions menu */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 min-w-0 overflow-hidden">
          {entry.tags.slice(0, MAX_VISIBLE_TAGS).map((tag) => (
            <Badge
              key={tag}
              asChild
              variant="secondary"
              appearance="light"
              size="xs"
              className={cn(
                "cursor-pointer transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                activeTag === tag
                  ? "bg-zinc-700 text-zinc-100"
                  : "text-zinc-500 hover:text-zinc-300"
              )}
            >
              <button
                type="button"
                onClick={() => onTagClick(tag)}
                aria-pressed={activeTag === tag}
                title={
                  activeTag === tag
                    ? `Clear tag filter "${tag}"`
                    : `Filter by tag "${tag}"`
                }
              >
                {tag}
              </button>
            </Badge>
          ))}
          {entry.tags.length > MAX_VISIBLE_TAGS && (
            <span
              className="text-xs text-zinc-600 shrink-0"
              title={entry.tags.slice(MAX_VISIBLE_TAGS).join(", ")}
            >
              +{entry.tags.length - MAX_VISIBLE_TAGS}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1.5 shrink-0">
          {entry.bead && (
            <button
              type="button"
              onClick={() => onNavigate?.(entry.bead)}
              className={cn(
                "text-xs font-mono text-zinc-500 hover:text-zinc-300 transition-colors rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring truncate max-w-[120px]",
                !onNavigate && "pointer-events-none"
              )}
              title={entry.bead}
              aria-label={`Navigate to bead ${entry.bead}`}
            >
              {formatMemoryBeadId(entry.bead)}
            </button>
          )}

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className="size-6 flex items-center justify-center rounded text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                aria-label="Entry actions"
              >
                <MoreVertical className="size-3.5" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              className="bg-zinc-900 border-zinc-800"
            >
              <DropdownMenuItem
                onClick={() => onEdit(entry)}
                className="text-zinc-200 focus:bg-zinc-800 focus:text-zinc-100 gap-2"
              >
                <Pencil className="size-3.5" aria-hidden="true" />
                Edit content
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => onEdit(entry)}
                className="text-zinc-200 focus:bg-zinc-800 focus:text-zinc-100 gap-2"
              >
                <Tag className="size-3.5" aria-hidden="true" />
                Edit tags
              </DropdownMenuItem>
              {entry.bead && onNavigate && (
                <DropdownMenuItem
                  onClick={() => onNavigate(entry.bead)}
                  className="text-zinc-200 focus:bg-zinc-800 focus:text-zinc-100 gap-2"
                >
                  <ExternalLink className="size-3.5" aria-hidden="true" />
                  Navigate to bead
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator className="bg-zinc-800" />
              <DropdownMenuItem
                onClick={() => onArchive(entry.key)}
                className="text-zinc-200 focus:bg-zinc-800 focus:text-zinc-100 gap-2"
              >
                <Archive className="size-3.5" aria-hidden="true" />
                Archive
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => onDelete(entry.key)}
                className="text-red-400 focus:bg-zinc-800 focus:text-red-400 gap-2"
              >
                <Trash2 className="size-3.5" aria-hidden="true" />
                Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </div>
  );
}

/**
 * Memory Panel - slide-out Sheet for browsing and managing knowledge base entries
 */
export function MemoryPanel({
  open,
  onOpenChange,
  projectPath,
  onNavigateToBead,
}: MemoryPanelProps) {
  const {
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
  } = useMemory(projectPath);

  // Tab state maps to type filter
  const [activeTab, setActiveTab] = useState<TabFilter>("all");

  // Add dialog state
  const [addOpen, setAddOpen] = useState(false);
  const [addContent, setAddContent] = useState("");
  const [addType, setAddType] = useState<MemoryType>("learned");
  const [addTags, setAddTags] = useState("");
  const [addBead, setAddBead] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  // Which memory system is shown (knowledge.jsonl vs bd KV store)
  const [system, setSystem] = useState<MemorySystem>("knowledge");
  // Facet filter row visibility
  const [showFacetFilters, setShowFacetFilters] = useState(false);

  // Number of active facet filters (beyond search + type tabs)
  const facetFilterCount =
    (sourceFilter ? 1 : 0) + (tagFilter ? 1 : 0) + (since ? 1 : 0) + (until ? 1 : 0);

  const hasActiveFilters = Boolean(search || typeFilter) || facetFilterCount > 0;

  /**
   * Facet options for the source/tag dropdowns, with counts.
   * Keeps the current selection listed even if it disappears from facets.
   */
  const sourceOptions = useMemo(() => {
    const counts = facets?.sources ?? {};
    const names = Object.keys(counts).sort((a, b) => a.localeCompare(b));
    if (sourceFilter && !(sourceFilter in counts)) names.unshift(sourceFilter);
    return names.map((name) => ({ name, count: counts[name] ?? 0 }));
  }, [facets, sourceFilter]);

  const tagOptions = useMemo(() => {
    const counts = facets?.tags ?? {};
    const names = Object.keys(counts).sort((a, b) => a.localeCompare(b));
    if (tagFilter && !(tagFilter in counts)) names.unshift(tagFilter);
    return names.map((name) => ({ name, count: counts[name] ?? 0 }));
  }, [facets, tagFilter]);

  /**
   * Reset every filter (search, type tab, and facets)
   */
  const clearAllFilters = useCallback(() => {
    setSearch("");
    setActiveTab("all");
    setTypeFilter(null);
    setSourceFilter(null);
    setTagFilter(null);
    setSince(null);
    setUntil(null);
  }, [setSearch, setTypeFilter, setSourceFilter, setTagFilter, setSince, setUntil]);

  /**
   * Entry types to offer as filter tabs. The canonical types are always
   * shown; additional known types (decision, gotcha, convention) and any
   * custom types appear once at least one entry uses them.
   */
  const typeTabs = useMemo(() => {
    const present = new Set<string>(entries.map((e) => e.type));
    const known = KNOWN_MEMORY_TYPES.filter(
      (t) =>
        (CANONICAL_MEMORY_TYPES as readonly string[]).includes(t) ||
        present.has(t)
    );
    const custom = Array.from(present)
      .filter((t) => !(KNOWN_MEMORY_TYPES as readonly string[]).includes(t))
      .sort();
    return [...known, ...custom];
  }, [entries]);

  // If the active tab's type disappears (e.g. last entry deleted), reset
  useEffect(() => {
    if (activeTab !== "all" && !typeTabs.includes(activeTab)) {
      setActiveTab("all");
      setTypeFilter(null);
    }
  }, [activeTab, typeTabs, setTypeFilter]);

  // Edit dialog state
  const [editingEntry, setEditingEntry] = useState<MemoryEntry | null>(null);
  const [editContent, setEditContent] = useState("");
  const [editTags, setEditTags] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  // Delete confirmation state
  const [deletingKey, setDeletingKey] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  /**
   * Handle tab change - maps tab value to type filter
   */
  const handleTabChange = useCallback(
    (value: string) => {
      const tabValue = value as TabFilter;
      setActiveTab(tabValue);
      setTypeFilter(tabValue === "all" ? null : tabValue);
    },
    [setTypeFilter]
  );

  /**
   * Handle tag chip click - toggles the tag filter
   */
  const handleTagClick = useCallback(
    (tag: string) => {
      setTagFilter(tagFilter === tag ? null : tag);
    },
    [tagFilter, setTagFilter]
  );

  /**
   * Handle navigate to bead
   */
  const handleNavigate = useCallback(
    (beadId: string) => {
      onOpenChange(false);
      onNavigateToBead?.(beadId);
    },
    [onOpenChange, onNavigateToBead]
  );

  /**
   * Open the add-entry dialog with a clean slate
   */
  const handleAddOpen = useCallback(() => {
    setAddContent("");
    setAddType("learned");
    setAddTags("");
    setAddBead("");
    setAddError(null);
    setAddOpen(true);
  }, []);

  /**
   * Save a new entry. The hook performs an optimistic insert and rolls
   * back on failure; on error the dialog stays open so typed content
   * is not lost.
   */
  const handleAddSave = useCallback(async () => {
    const content = addContent.trim();
    if (!content) return;
    setIsAdding(true);
    setAddError(null);
    try {
      const tags = addTags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      await addEntry(content, addType, tags, addBead.trim() || undefined);
      setAddOpen(false);
    } catch (err) {
      setAddError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsAdding(false);
    }
  }, [addContent, addType, addTags, addBead, addEntry]);

  /**
   * Open edit dialog for an entry
   */
  const handleEditOpen = useCallback((entry: MemoryEntry) => {
    setEditingEntry(entry);
    setEditContent(entry.content);
    setEditTags(entry.tags.join(", "));
  }, []);

  /**
   * Save edited entry
   */
  const handleEditSave = useCallback(async () => {
    if (!editingEntry) return;
    setIsSaving(true);
    try {
      const newTags = editTags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      await editEntry(editingEntry.key, editContent, newTags);
      setEditingEntry(null);
    } catch {
      // Error is logged in hook
    } finally {
      setIsSaving(false);
    }
  }, [editingEntry, editContent, editTags, editEntry]);

  /**
   * Handle archive
   */
  const handleArchive = useCallback(
    async (key: string) => {
      try {
        await archiveEntry(key);
      } catch {
        // Error is logged in hook
      }
    },
    [archiveEntry]
  );

  /**
   * Handle delete confirmation
   */
  const handleDeleteConfirm = useCallback(async () => {
    if (!deletingKey) return;
    setIsDeleting(true);
    try {
      await deleteEntry(deletingKey);
      setDeletingKey(null);
    } catch {
      // Error is logged in hook
    } finally {
      setIsDeleting(false);
    }
  }, [deletingKey, deleteEntry]);

  return (
    <>
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent
          side="right"
          className="w-full sm:max-w-lg md:max-w-xl bg-[#0a0a0a] border-zinc-800 flex flex-col"
        >
          <SheetHeader className="space-y-1">
            <SheetTitle className="flex items-center gap-2 text-zinc-100">
              <BrainCircuit className="size-5" aria-hidden="true" />
              Memory
            </SheetTitle>
            <SheetDescription className="text-zinc-500">
              {system === "kv"
                ? "bd CLI key→value store — separate from knowledge.jsonl"
                : stats
                  ? `${stats.total} ${stats.total === 1 ? "entry" : "entries"}`
                  : "Loading..."}
            </SheetDescription>
          </SheetHeader>

          {/* Memory system switcher: knowledge.jsonl (System B) vs upstream bd KV memory (System A) */}
          <Tabs
            value={system}
            onValueChange={(v) => setSystem(v as MemorySystem)}
            className="mt-4"
          >
            <TabsList className="h-8 bg-zinc-800/50 p-0.5 w-full">
              <TabsTrigger
                value="knowledge"
                className="h-7 flex-1 text-sm font-medium data-[state=active]:bg-zinc-700 data-[state=active]:text-zinc-100 data-[state=inactive]:text-zinc-400"
              >
                Knowledge
              </TabsTrigger>
              <TabsTrigger
                value="kv"
                className="h-7 flex-1 text-sm font-medium data-[state=active]:bg-zinc-700 data-[state=active]:text-zinc-100 data-[state=inactive]:text-zinc-400"
              >
                CLI Memory (bd)
              </TabsTrigger>
            </TabsList>
          </Tabs>

          {system === "kv" ? (
            <BdMemoryTab projectPath={projectPath} />
          ) : (
            <>
              {/* Search + add entry row */}
              <div className="mt-4 flex items-center gap-2">
                <div className="relative flex-1">
                  <Search
                    className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-zinc-500"
                    aria-hidden="true"
                  />
                  <Input
                    type="text"
                    aria-label="Search memories"
                    placeholder="Search memories..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="pl-8 pr-8 h-8 bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500"
                  />
                  {search && (
                    <button
                      type="button"
                      onClick={() => setSearch("")}
                      className="absolute right-0 top-1/2 -translate-y-1/2 size-11 flex items-center justify-center text-zinc-500 hover:text-zinc-300"
                      aria-label="Clear search"
                    >
                      <X className="size-3.5" />
                    </button>
                  )}
                </div>
                <Button
                  size="sm"
                  onClick={handleAddOpen}
                  className="shrink-0"
                  aria-label="Add knowledge entry"
                >
                  <Plus className="size-3.5" aria-hidden="true" />
                  Add
                </Button>
              </div>

              {/* Facet filters (source / tag / date range) */}
              <div className="flex items-center justify-between mt-2">
                <button
                  type="button"
                  onClick={() => setShowFacetFilters((v) => !v)}
                  aria-expanded={showFacetFilters}
                  className="flex items-center gap-1.5 text-xs text-zinc-400 hover:text-zinc-200 transition-colors rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <SlidersHorizontal className="size-3.5" aria-hidden="true" />
                  Filters
                  {facetFilterCount > 0 && (
                    <Badge variant="secondary" appearance="light" size="xs">
                      {facetFilterCount}
                    </Badge>
                  )}
                </button>
                {hasActiveFilters && (
                  <button
                    type="button"
                    onClick={clearAllFilters}
                    className="text-xs text-zinc-500 hover:text-zinc-300 underline underline-offset-2 rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                  >
                    Clear all
                  </button>
                )}
              </div>

              {showFacetFilters && (
                <div className="mt-2 grid grid-cols-2 gap-2">
                  <Select
                    value={sourceFilter ?? "all"}
                    onValueChange={(value) =>
                      setSourceFilter(value === "all" ? null : value)
                    }
                  >
                    <SelectTrigger
                      aria-label="Filter by source"
                      className="h-8 text-xs bg-zinc-800/50 border-zinc-700 text-zinc-100"
                    >
                      <SelectValue placeholder="Source" />
                    </SelectTrigger>
                    <SelectContent className="bg-zinc-900 border-zinc-800 text-zinc-200">
                      <SelectItem value="all">All sources</SelectItem>
                      {sourceOptions.map(({ name, count }) => (
                        <SelectItem key={name} value={name}>
                          {name} ({count})
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  <Select
                    value={tagFilter ?? "all"}
                    onValueChange={(value) =>
                      setTagFilter(value === "all" ? null : value)
                    }
                  >
                    <SelectTrigger
                      aria-label="Filter by tag"
                      className="h-8 text-xs bg-zinc-800/50 border-zinc-700 text-zinc-100"
                    >
                      <SelectValue placeholder="Tag" />
                    </SelectTrigger>
                    <SelectContent className="bg-zinc-900 border-zinc-800 text-zinc-200">
                      <SelectItem value="all">All tags</SelectItem>
                      {tagOptions.map(({ name, count }) => (
                        <SelectItem key={name} value={name}>
                          {name} ({count})
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>

                  <Input
                    type="date"
                    aria-label="From date"
                    value={since ?? ""}
                    onChange={(e) => setSince(e.target.value || null)}
                    className="h-8 text-xs bg-zinc-800/50 border-zinc-700 text-zinc-100 [color-scheme:dark]"
                  />
                  <Input
                    type="date"
                    aria-label="To date"
                    value={until ?? ""}
                    onChange={(e) => setUntil(e.target.value || null)}
                    className="h-8 text-xs bg-zinc-800/50 border-zinc-700 text-zinc-100 [color-scheme:dark]"
                  />
                </div>
              )}

              {/* Active tag filter */}
              {tagFilter && (
                <div className="mt-2 flex items-center gap-1.5">
                  <Tag className="size-3.5 text-zinc-500" aria-hidden="true" />
                  <span className="text-xs text-zinc-500">Filtered by tag</span>
                  <Badge
                    variant="secondary"
                    appearance="light"
                    size="xs"
                    className="text-zinc-300 gap-1"
                  >
                    {tagFilter}
                    <button
                      type="button"
                      onClick={() => setTagFilter(null)}
                      className="rounded hover:text-zinc-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                      aria-label={`Clear tag filter ${tagFilter}`}
                    >
                      <X className="size-3" />
                    </button>
                  </Badge>
                </div>
              )}

              {/* Type filter tabs */}
              <Tabs
                value={activeTab}
                onValueChange={handleTabChange}
                className="mt-3"
              >
                <TabsList className="h-8 bg-zinc-800/50 p-0.5 w-full">
                  <TabsTrigger value="all" className={TAB_TRIGGER_CLASSES}>
                    All
                  </TabsTrigger>
                  {typeTabs.map((type) => (
                    <TabsTrigger
                      key={type}
                      value={type}
                      className={TAB_TRIGGER_CLASSES}
                    >
                      {formatTypeLabel(type)}
                    </TabsTrigger>
                  ))}
                </TabsList>
              </Tabs>

              {/* Entries list */}
              <ScrollArea className="flex-1 mt-3 -mx-6 px-6">
                <div className="space-y-2 pb-4">
                  {isLoading ? (
                    <div className="flex items-center justify-center py-12">
                      <Loader2 className="size-5 text-zinc-500 animate-spin" aria-hidden="true" />
                      <span className="sr-only">Loading memory entries</span>
                    </div>
                  ) : error ? (
                    <div
                      role="alert"
                      className="rounded-lg border border-red-500/30 bg-red-500/10 p-4 text-center"
                    >
                      <p className="text-sm text-red-400">
                        Failed to load memory entries
                      </p>
                      <p className="text-xs text-red-400/60 mt-1">
                        {error.message}
                      </p>
                    </div>
                  ) : filteredEntries.length === 0 ? (
                    <div className="flex flex-col items-center justify-center py-12 text-center">
                      <BrainCircuit
                        className="size-8 text-zinc-700 mb-3"
                        aria-hidden="true"
                      />
                      <p className="text-sm text-zinc-500">
                        {hasActiveFilters
                          ? "No entries match your search"
                          : "No memory entries yet"}
                      </p>
                      {hasActiveFilters ? (
                        <button
                          type="button"
                          onClick={clearAllFilters}
                          className="mt-2 text-xs text-zinc-500 hover:text-zinc-300 underline underline-offset-2 rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                        >
                          Clear filters
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={handleAddOpen}
                          className="mt-2 text-xs text-zinc-500 hover:text-zinc-300 underline underline-offset-2 rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                        >
                          Add your first entry
                        </button>
                      )}
                    </div>
                  ) : (
                    filteredEntries.map((entry) => (
                      <MemoryEntryCard
                        key={entry.key}
                        entry={entry}
                        activeTag={tagFilter}
                        onTagClick={handleTagClick}
                        onEdit={handleEditOpen}
                        onArchive={handleArchive}
                        onDelete={setDeletingKey}
                        onNavigate={onNavigateToBead ? handleNavigate : undefined}
                      />
                    ))
                  )}
                </div>
              </ScrollArea>

              {/* Footer stats */}
              {stats && stats.total > 0 && (
                <SheetFooter className="border-t border-zinc-800 pt-3 -mx-6 px-6">
                  <p className="text-xs text-zinc-600 w-full text-center">
                    {typeTabs
                      .filter((type) => (stats.by_type[type] ?? 0) > 0)
                      .map((type, index) => (
                        <span key={type}>
                          {index > 0 && (
                            <span className="mx-1.5" aria-hidden="true">
                              ·
                            </span>
                          )}
                          <span className="tabular-nums">
                            {stats.by_type[type]}
                          </span>{" "}
                          {type}
                        </span>
                      ))}
                    {stats.archived > 0 && (
                      <>
                        <span className="mx-1.5" aria-hidden="true">
                          ·
                        </span>
                        <span className="tabular-nums">{stats.archived}</span>{" "}
                        archived
                      </>
                    )}
                  </p>
                </SheetFooter>
              )}
            </>
          )}
        </SheetContent>
      </Sheet>

      {/* Add Knowledge Dialog */}
      <AlertDialog
        open={addOpen}
        onOpenChange={(isOpen) => {
          if (!isOpen && !isAdding) setAddOpen(false);
        }}
      >
        <AlertDialogContent className="bg-zinc-900 border-zinc-800">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-zinc-100">
              Add Knowledge
            </AlertDialogTitle>
            <AlertDialogDescription className="text-zinc-500">
              Capture knowledge for this project — something learned,
              investigation context, a decision, gotcha, or convention.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <label
                htmlFor="add-content"
                className="text-sm font-medium text-zinc-300"
              >
                Content
              </label>
              <textarea
                id="add-content"
                value={addContent}
                onChange={(e) => setAddContent(e.target.value)}
                placeholder="What did you learn or discover?"
                autoFocus
                className="w-full h-32 rounded-md border border-zinc-700 bg-zinc-800/50 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
              />
            </div>
            <div className="space-y-2">
              <span className="text-sm font-medium text-zinc-300">Type</span>
              <div
                className="flex flex-wrap gap-2"
                role="radiogroup"
                aria-label="Entry type"
              >
                {KNOWN_MEMORY_TYPES.map((t) => (
                  <button
                    key={t}
                    type="button"
                    role="radio"
                    aria-checked={addType === t}
                    onClick={() => setAddType(t)}
                    className={cn(
                      "h-8 px-3 rounded-md border text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                      addType === t
                        ? "border-zinc-600 bg-zinc-700 text-zinc-100"
                        : "border-zinc-700 bg-zinc-800/50 text-zinc-400 hover:text-zinc-200"
                    )}
                  >
                    {formatTypeLabel(t)}
                  </button>
                ))}
              </div>
            </div>
            <div className="space-y-2">
              <label
                htmlFor="add-tags"
                className="text-sm font-medium text-zinc-300"
              >
                Tags (comma-separated)
              </label>
              <Input
                id="add-tags"
                value={addTags}
                onChange={(e) => setAddTags(e.target.value)}
                className="bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500"
                placeholder="tag1, tag2, tag3..."
              />
            </div>
            <div className="space-y-2">
              <label
                htmlFor="add-bead"
                className="text-sm font-medium text-zinc-300"
              >
                Linked bead (optional)
              </label>
              <Input
                id="add-bead"
                value={addBead}
                onChange={(e) => setAddBead(e.target.value)}
                className="bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500 font-mono"
                placeholder="e.g. sweetgrass-0e9"
              />
            </div>
            {addError && (
              <p role="alert" className="text-xs text-red-400">
                Failed to add entry: {addError}
              </p>
            )}
          </div>
          <AlertDialogFooter>
            <AlertDialogClose render={<Button variant="ghost">Cancel</Button>} />
            <Button
              onClick={handleAddSave}
              disabled={isAdding || !addContent.trim()}
            >
              {isAdding ? (
                <Loader2 className="size-4 animate-spin mr-1.5" aria-hidden="true" />
              ) : (
                <Plus className="size-4 mr-1.5" aria-hidden="true" />
              )}
              Add entry
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Edit Dialog */}
      <AlertDialog
        open={!!editingEntry}
        onOpenChange={(isOpen) => !isOpen && setEditingEntry(null)}
      >
        <AlertDialogContent className="bg-zinc-900 border-zinc-800">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-zinc-100">
              Edit Memory Entry
            </AlertDialogTitle>
            <AlertDialogDescription className="text-zinc-500">
              Update the content or tags for this entry.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <div className="space-y-4 py-2">
            <div className="space-y-2">
              <label
                htmlFor="edit-content"
                className="text-sm font-medium text-zinc-300"
              >
                Content
              </label>
              <textarea
                id="edit-content"
                value={editContent}
                onChange={(e) => setEditContent(e.target.value)}
                className="w-full h-32 rounded-md border border-zinc-700 bg-zinc-800/50 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
              />
            </div>
            <div className="space-y-2">
              <label
                htmlFor="edit-tags"
                className="text-sm font-medium text-zinc-300"
              >
                Tags (comma-separated)
              </label>
              <Input
                id="edit-tags"
                value={editTags}
                onChange={(e) => setEditTags(e.target.value)}
                className="bg-zinc-800/50 border-zinc-700 text-zinc-100 placeholder:text-zinc-500"
                placeholder="tag1, tag2, tag3..."
              />
            </div>
          </div>
          <AlertDialogFooter>
            <AlertDialogClose render={<Button variant="ghost">Cancel</Button>} />
            <Button onClick={handleEditSave} disabled={isSaving}>
              {isSaving ? (
                <Loader2 className="size-4 animate-spin mr-1.5" aria-hidden="true" />
              ) : null}
              Save
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Delete Confirmation Dialog */}
      <AlertDialog
        open={!!deletingKey}
        onOpenChange={(isOpen) => !isOpen && setDeletingKey(null)}
      >
        <AlertDialogContent className="bg-zinc-900 border-zinc-800">
          <AlertDialogHeader>
            <AlertDialogTitle className="text-zinc-100">
              Delete Memory Entry
            </AlertDialogTitle>
            <AlertDialogDescription className="text-zinc-400">
              This will permanently delete this memory entry. This action cannot
              be undone. Consider archiving instead.
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
                <Loader2 className="size-4 animate-spin mr-1.5" aria-hidden="true" />
              ) : (
                <Trash2 className="size-4 mr-1.5" aria-hidden="true" />
              )}
              Delete
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
