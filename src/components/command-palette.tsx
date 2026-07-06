"use client";

import { FolderKanban, Layers, Search, Circle, LayoutGrid, Rows3 } from "lucide-react";
import { useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import * as api from "@/lib/api";
import { formatBeadId, statusDotColor } from "@/lib/bead-display";
import { cn } from "@/lib/utils";
import type { Bead, Project } from "@/types";

type Command =
  | { kind: "project"; id: string; label: string; sublabel: string; run: () => void }
  | { kind: "bead"; id: string; label: string; sublabel: string; status: Bead["status"]; run: () => void }
  | { kind: "action"; id: string; label: string; sublabel: string; run: () => void };

/**
 * Global command palette (Cmd/Ctrl+K). Self-contained — no cmdk dependency.
 * Jumps to any project, any bead in the current project, and toggles views.
 */
export function CommandPalette() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const projectId = searchParams.get("id");

  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [projects, setProjects] = useState<Project[]>([]);
  const [beads, setBeads] = useState<Bead[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Global Cmd+K / Ctrl+K listener.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Load projects when opened; current project's beads load in the next effect.
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
    api.projects
      .list()
      .then(setProjects)
      .catch(() => setProjects([]));
  }, [open]);

  // Once projects are loaded and we have a current projectId, load its beads.
  useEffect(() => {
    if (!open || !projectId || projects.length === 0) return;
    const proj = projects.find((p) => p.id === projectId);
    if (!proj) return;
    api.beads
      .read(proj.path)
      .then((res) => setBeads(res.beads))
      .catch(() => setBeads([]));
  }, [open, projectId, projects]);

  const close = useCallback(() => setOpen(false), []);

  const commands = useMemo<Command[]>(() => {
    const cmds: Command[] = [];

    // View actions (only meaningful on a project page)
    if (projectId) {
      cmds.push({
        kind: "action",
        id: "view-board",
        label: "Switch to Board view",
        sublabel: "Kanban",
        run: () => router.replace(`/project?id=${projectId}`),
      });
      cmds.push({
        kind: "action",
        id: "view-table",
        label: "Switch to Table view",
        sublabel: "Table",
        run: () => router.replace(`/project?id=${projectId}&view=table`),
      });
    }

    // Projects
    for (const p of projects) {
      cmds.push({
        kind: "project",
        id: `proj-${p.id}`,
        label: p.name,
        sublabel: "Project",
        run: () => router.push(`/project?id=${p.id}`),
      });
    }

    // Beads in the current project
    for (const b of beads) {
      const isEpic = b.issue_type === "epic";
      cmds.push({
        kind: "bead",
        id: `bead-${b.id}`,
        status: b.status,
        label: b.title,
        sublabel: `${isEpic ? "Epic " : ""}${formatBeadId(b.id)}`,
        run: () => {
          if (isEpic && projectId) {
            router.push(`/epic?id=${projectId}&epic=${encodeURIComponent(b.id)}`);
          } else if (projectId) {
            // Open the board and let the user find it; deep-linking to a bead
            // detail isn't a route, so navigate to the board scoped to search.
            router.push(`/project?id=${projectId}`);
          }
        },
      });
    }

    return cmds;
  }, [projectId, projects, beads, router]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands.slice(0, 50);
    const scored = commands
      .map((c) => {
        const hay = `${c.label} ${c.sublabel}`.toLowerCase();
        const idx = hay.indexOf(q);
        return idx === -1 ? null : { c, idx };
      })
      .filter((x): x is { c: Command; idx: number } => x !== null)
      .sort((a, b) => a.idx - b.idx)
      .slice(0, 50)
      .map((x) => x.c);
    return scored;
  }, [commands, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  const runActive = useCallback(() => {
    const cmd = filtered[activeIndex];
    if (cmd) {
      cmd.run();
      setOpen(false);
    }
  }, [filtered, activeIndex]);

  const onInputKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActiveIndex((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActiveIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      runActive();
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="max-w-xl gap-0 overflow-hidden border-zinc-800 bg-zinc-950 p-0"
        onOpenAutoFocus={(e) => {
          e.preventDefault();
          inputRef.current?.focus();
        }}
      >
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <div className="flex items-center gap-2 border-b border-zinc-800 px-3">
          <Search className="size-4 flex-shrink-0 text-zinc-500" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onInputKey}
            placeholder="Search projects, epics, beads…"
            className="h-12 w-full bg-transparent text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none"
          />
        </div>
        <div ref={listRef} className="max-h-80 overflow-y-auto p-1.5">
          {filtered.length === 0 ? (
            <div className="px-3 py-6 text-center text-sm text-zinc-600">
              No matches.
            </div>
          ) : (
            filtered.map((cmd, i) => (
              <button
                key={cmd.id}
                onClick={() => {
                  cmd.run();
                  close();
                }}
                onMouseEnter={() => setActiveIndex(i)}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm",
                  i === activeIndex ? "bg-zinc-800 text-zinc-100" : "text-zinc-300"
                )}
              >
                <CommandIcon cmd={cmd} />
                <span className="min-w-0 flex-1 truncate">{cmd.label}</span>
                <span className="flex-shrink-0 text-xs text-zinc-600">
                  {cmd.sublabel}
                </span>
              </button>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function CommandIcon({ cmd }: { cmd: Command }) {
  if (cmd.kind === "project")
    return <FolderKanban className="size-4 flex-shrink-0 text-blue-400" />;
  if (cmd.kind === "action")
    return cmd.id === "view-table" ? (
      <Rows3 className="size-4 flex-shrink-0 text-zinc-400" />
    ) : (
      <LayoutGrid className="size-4 flex-shrink-0 text-zinc-400" />
    );
  // bead
  if (cmd.sublabel.startsWith("Epic"))
    return <Layers className="size-4 flex-shrink-0 text-purple-400" />;
  return (
    <Circle
      className={cn(
        "size-3 flex-shrink-0 fill-current",
        statusDotColor(cmd.status)
      )}
    />
  );
}
