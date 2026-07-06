"use client";

import {
  ArrowLeft,
  ChevronRight,
  Circle,
  FileText,
  AlertTriangle,
  LayoutGrid,
} from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useMemo, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useBeads } from "@/hooks/use-beads";
import { useProject } from "@/hooks/use-project";
import {
  beadDepth,
  formatBeadId,
  isBlocked,
  priorityColor,
  priorityLabel,
  statusDotColor,
  statusLabel,
  statusTextColor,
} from "@/lib/bead-display";
import { cn } from "@/lib/utils";
import type { Bead } from "@/types";

/** Recursively collect an epic's descendants in hierarchical order. */
function collectDescendants(
  rootId: string,
  byParent: Map<string, Bead[]>
): Bead[] {
  const out: Bead[] = [];
  const walk = (parentId: string) => {
    const kids = byParent.get(parentId) ?? [];
    for (const kid of kids) {
      out.push(kid);
      walk(kid.id);
    }
  };
  walk(rootId);
  return out;
}

export function EpicDetailView() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const projectId = searchParams.get("id");
  const epicId = searchParams.get("epic");

  const { project, isLoading: projectLoading } = useProject(projectId);
  const { beads, isLoading: beadsLoading } = useBeads(project?.path ?? "");

  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const beadsById = useMemo(() => {
    const m = new Map<string, Bead>();
    for (const b of beads) m.set(b.id, b);
    return m;
  }, [beads]);

  const byParent = useMemo(() => {
    const m = new Map<string, Bead[]>();
    for (const b of beads) {
      if (b.parent_id) {
        const arr = m.get(b.parent_id) ?? [];
        arr.push(b);
        m.set(b.parent_id, arr);
      }
    }
    return m;
  }, [beads]);

  const epic = epicId ? beadsById.get(epicId) : undefined;

  const descendants = useMemo(
    () => (epic ? collectDescendants(epic.id, byParent) : []),
    [epic, byParent]
  );

  const progress = useMemo(() => {
    const total = descendants.length;
    const completed = descendants.filter((b) => b.status === "closed").length;
    const inProgress = descendants.filter(
      (b) => b.status === "in_progress"
    ).length;
    const blocked = descendants.filter((b) => isBlocked(b)).length;
    const pct = total > 0 ? Math.round((completed / total) * 100) : 0;
    return { total, completed, inProgress, blocked, pct };
  }, [descendants]);

  // Rows visible given collapse state: hide a bead if any ancestor is collapsed.
  const visibleRows = useMemo(() => {
    if (!epic) return [];
    const isHidden = (b: Bead): boolean => {
      let p = b.parent_id;
      while (p && p !== epic.id) {
        if (collapsed.has(p)) return true;
        p = beadsById.get(p)?.parent_id;
      }
      return collapsed.has(epic.id) ? true : false;
    };
    return descendants.filter((b) => !isHidden(b));
  }, [descendants, collapsed, epic, beadsById]);

  const baseDepth = epic ? beadDepth(epic.id) + 1 : 0;

  if (projectLoading || beadsLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-[#0a0a0a] text-zinc-400">
        Loading epic…
      </div>
    );
  }

  if (!project || !epic) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-4 bg-[#0a0a0a] text-zinc-400">
        <p>Epic not found.</p>
        <Link href={projectId ? `/project?id=${projectId}` : "/"}>
          <Button variant="outline" size="sm">
            <ArrowLeft className="size-4" /> Back to board
          </Button>
        </Link>
      </div>
    );
  }

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="min-h-screen bg-[#0a0a0a] text-zinc-100">
      <div className="mx-auto max-w-6xl px-6 py-6">
        {/* Breadcrumb / header */}
        <div className="mb-4 flex items-center gap-3 text-sm text-zinc-400">
          <Link
            href={`/project?id=${projectId}`}
            className="flex items-center gap-1 hover:text-zinc-200"
          >
            <ArrowLeft className="size-4" />
            {project.name}
          </Link>
          <span className="text-zinc-600">/</span>
          <span className="font-mono text-zinc-500">{formatBeadId(epic.id)}</span>
        </div>

        <div className="mb-6 flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h1 className="text-2xl font-semibold leading-tight text-zinc-50">
              {epic.title}
            </h1>
            {epic.description && (
              <p className="mt-2 max-w-3xl text-sm leading-relaxed text-zinc-400">
                {epic.description.split("\n")[0]}
              </p>
            )}
          </div>
          <div className="flex flex-shrink-0 items-center gap-2">
            {epic.design_doc && (
              <Badge
                variant="outline"
                className="border-zinc-700 text-zinc-300"
              >
                <FileText className="size-3" /> Design doc
              </Badge>
            )}
            <Link href={`/project?id=${projectId}`}>
              <Button variant="outline" size="sm">
                <LayoutGrid className="size-4" /> Board
              </Button>
            </Link>
          </div>
        </div>

        {/* Progress summary */}
        <div className="mb-6 rounded-lg border border-zinc-800 bg-zinc-900/40 p-4">
          <div className="mb-2 flex items-center justify-between text-sm">
            <span className="text-zinc-300">
              {progress.completed}/{progress.total} closed ({progress.pct}%)
            </span>
            <div className="flex items-center gap-4 text-xs">
              <span className="text-blue-400">
                {progress.inProgress} in progress
              </span>
              {progress.blocked > 0 && (
                <span className="flex items-center gap-1 text-red-400">
                  <AlertTriangle className="size-3" /> {progress.blocked} blocked
                </span>
              )}
              <span className={priorityColor(epic.priority)}>
                {priorityLabel(epic.priority)}
              </span>
            </div>
          </div>
          <Progress value={progress.pct} className="h-2" />
        </div>

        {/* Issue table */}
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-[42%]">Issue</TableHead>
              <TableHead className="w-[120px]">Status</TableHead>
              <TableHead className="w-[70px]">Priority</TableHead>
              <TableHead className="w-[90px]">Deps</TableHead>
              <TableHead>Owner</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {visibleRows.map((bead) => {
              const depth = Math.max(0, beadDepth(bead.id) - baseDepth);
              const kids = byParent.get(bead.id) ?? [];
              const hasKids = kids.length > 0;
              const isCollapsed = collapsed.has(bead.id);
              const blocked = isBlocked(bead);
              return (
                <TableRow key={bead.id} className="cursor-default">
                  <TableCell>
                    <div
                      className="flex items-center gap-1.5"
                      style={{ paddingLeft: `${depth * 18}px` }}
                    >
                      {hasKids ? (
                        <button
                          onClick={() => toggle(bead.id)}
                          className="rounded p-0.5 text-zinc-500 hover:text-zinc-200"
                          aria-label={isCollapsed ? "Expand" : "Collapse"}
                        >
                          <ChevronRight
                            className={cn(
                              "size-3.5 transition-transform",
                              !isCollapsed && "rotate-90"
                            )}
                          />
                        </button>
                      ) : (
                        <span className="w-[18px]" />
                      )}
                      <Circle
                        className={cn(
                          "size-2 flex-shrink-0 fill-current",
                          statusDotColor(bead.status)
                        )}
                        aria-hidden="true"
                      />
                      <span className="truncate text-zinc-200">
                        {bead.title}
                      </span>
                      <span className="ml-1 flex-shrink-0 font-mono text-xs text-zinc-600">
                        {formatBeadId(bead.id)}
                      </span>
                    </div>
                  </TableCell>
                  <TableCell>
                    <span className={cn("text-xs", statusTextColor(bead.status))}>
                      {statusLabel(bead.status)}
                    </span>
                  </TableCell>
                  <TableCell>
                    <span className={cn("text-xs", priorityColor(bead.priority))}>
                      {priorityLabel(bead.priority)}
                    </span>
                  </TableCell>
                  <TableCell>
                    {blocked ? (
                      <Popover>
                        <PopoverTrigger asChild>
                          <button className="flex items-center gap-1 text-xs text-red-400 hover:text-red-300">
                            <AlertTriangle className="size-3" />
                            {(bead.deps ?? []).length}
                          </button>
                        </PopoverTrigger>
                        <PopoverContent
                          align="start"
                          className="w-72 border-zinc-800 bg-zinc-900 p-2 text-sm"
                        >
                          <p className="mb-1.5 px-1 text-xs font-medium text-zinc-400">
                            Blocked by
                          </p>
                          <div className="space-y-0.5">
                            {(bead.deps ?? []).map((depId) => {
                              const dep = beadsById.get(depId);
                              return (
                                <div
                                  key={depId}
                                  className="flex items-center gap-2 rounded px-1 py-1 hover:bg-zinc-800"
                                >
                                  <Circle
                                    className={cn(
                                      "size-2 flex-shrink-0 fill-current",
                                      dep
                                        ? statusDotColor(dep.status)
                                        : "text-zinc-600"
                                    )}
                                  />
                                  <span className="truncate text-zinc-300">
                                    {dep?.title ?? depId}
                                  </span>
                                  <span className="ml-auto flex-shrink-0 font-mono text-xs text-zinc-600">
                                    {formatBeadId(depId)}
                                  </span>
                                </div>
                              );
                            })}
                          </div>
                        </PopoverContent>
                      </Popover>
                    ) : (
                      <span className="text-xs text-zinc-600">—</span>
                    )}
                  </TableCell>
                  <TableCell>
                    <span className="truncate text-xs text-zinc-500">
                      {bead.owner?.split("@")[0] ?? "—"}
                    </span>
                  </TableCell>
                </TableRow>
              );
            })}
            {descendants.length === 0 && (
              <TableRow className="hover:bg-transparent">
                <TableCell colSpan={5} className="py-8 text-center text-zinc-500">
                  This epic has no child issues yet.
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
