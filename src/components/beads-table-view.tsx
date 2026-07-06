"use client";

import {
  AlertTriangle,
  ChevronRight,
  Circle,
  Layers,
  Maximize2,
} from "lucide-react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useMemo, useState } from "react";

import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
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

export type TableDensity = "comfortable" | "compact";

export interface BeadsTableViewProps {
  /**
   * The beads to display. In "epics" view these are epics (children resolved
   * from allBeads); in "tasks" view these are all tasks, rendered flat.
   */
  topLevelBeads: Bead[];
  /** All beads (for resolving epic children and parent-epic labels). */
  allBeads: Bead[];
  /** Which grain to render: epic-grouped, or a flat task list. */
  view: "epics" | "tasks";
  density: TableDensity;
  onSelectBead: (bead: Bead) => void;
}

interface EpicGroup {
  epic: Bead | null; // null = standalone tasks bucket
  children: Bead[];
  completed: number;
  blocked: number;
  pct: number;
}

export function BeadsTableView({
  topLevelBeads,
  allBeads,
  view,
  density,
  onSelectBead,
}: BeadsTableViewProps) {
  const searchParams = useSearchParams();
  const projectId = searchParams.get("id");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  // Resolve a task's parent-epic title for the flat task view.
  const epicTitleById = useMemo(() => {
    const m = new Map<string, string>();
    for (const b of allBeads) {
      if (b.issue_type === "epic") m.set(b.id, b.title);
    }
    return m;
  }, [allBeads]);

  const byParent = useMemo(() => {
    const m = new Map<string, Bead[]>();
    for (const b of allBeads) {
      if (b.parent_id) {
        const arr = m.get(b.parent_id) ?? [];
        arr.push(b);
        m.set(b.parent_id, arr);
      }
    }
    return m;
  }, [allBeads]);

  const groups = useMemo<EpicGroup[]>(() => {
    const epics = topLevelBeads.filter((b) => b.issue_type === "epic");
    const standalone = topLevelBeads.filter((b) => b.issue_type !== "epic");
    const result: EpicGroup[] = epics.map((epic) => {
      const children = byParent.get(epic.id) ?? [];
      const completed = children.filter((c) => c.status === "closed").length;
      const blocked = children.filter((c) => isBlocked(c)).length;
      const pct =
        children.length > 0
          ? Math.round((completed / children.length) * 100)
          : 0;
      return { epic, children, completed, blocked, pct };
    });
    if (standalone.length > 0) {
      const completed = standalone.filter((c) => c.status === "closed").length;
      const blocked = standalone.filter((c) => isBlocked(c)).length;
      result.push({
        epic: null,
        children: standalone,
        completed,
        blocked,
        pct:
          standalone.length > 0
            ? Math.round((completed / standalone.length) * 100)
            : 0,
      });
    }
    return result;
  }, [topLevelBeads, byParent]);

  const rowPad = density === "compact" ? "py-1" : "py-2";

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // Task view: a flat list of all tasks, each with an Epic column.
  if (view === "tasks") {
    return (
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-[40%]">Issue</TableHead>
            <TableHead className="w-[22%]">Epic</TableHead>
            <TableHead className="w-[110px]">Status</TableHead>
            <TableHead className="w-[70px]">Priority</TableHead>
            <TableHead className="w-[70px]">Deps</TableHead>
            <TableHead>Owner</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {topLevelBeads.map((bead) => {
            const blocked = isBlocked(bead);
            const epicTitle = bead.parent_id
              ? epicTitleById.get(bead.parent_id)
              : undefined;
            return (
              <TableRow
                key={bead.id}
                className="cursor-pointer"
                onClick={() => onSelectBead(bead)}
              >
                <TableCell className={rowPad}>
                  <div className="flex items-center gap-2">
                    <Circle
                      className={cn(
                        "size-2 flex-shrink-0 fill-current",
                        statusDotColor(bead.status)
                      )}
                      aria-hidden="true"
                    />
                    <span className="truncate text-zinc-200">{bead.title}</span>
                    <span className="ml-1 flex-shrink-0 font-mono text-xs text-zinc-600">
                      {formatBeadId(bead.id)}
                    </span>
                  </div>
                </TableCell>
                <TableCell className={rowPad}>
                  {epicTitle ? (
                    <span className="flex items-center gap-1 text-xs text-purple-300/80">
                      <Layers className="size-3 flex-shrink-0" />
                      <span className="truncate">{epicTitle}</span>
                    </span>
                  ) : (
                    <span className="text-xs text-zinc-600">—</span>
                  )}
                </TableCell>
                <TableCell className={rowPad}>
                  <span className={cn("text-xs", statusTextColor(bead.status))}>
                    {statusLabel(bead.status)}
                  </span>
                </TableCell>
                <TableCell className={rowPad}>
                  <span className={cn("text-xs", priorityColor(bead.priority))}>
                    {priorityLabel(bead.priority)}
                  </span>
                </TableCell>
                <TableCell className={rowPad}>
                  {blocked ? (
                    <span className="flex items-center gap-1 text-xs text-red-400">
                      <AlertTriangle className="size-3" />
                      {(bead.deps ?? []).length}
                    </span>
                  ) : (
                    <span className="text-xs text-zinc-600">—</span>
                  )}
                </TableCell>
                <TableCell className={rowPad}>
                  <span className="truncate text-xs text-zinc-500">
                    {bead.owner?.split("@")[0] ?? "—"}
                  </span>
                </TableCell>
              </TableRow>
            );
          })}
          {topLevelBeads.length === 0 && (
            <TableRow className="hover:bg-transparent">
              <TableCell colSpan={6} className="py-10 text-center text-zinc-500">
                No tasks match the current filters.
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    );
  }

  // Epic view: epic-grouped table.
  return (
    <Table>
      <TableHeader>
        <TableRow className="hover:bg-transparent">
          <TableHead className="w-[46%]">Issue</TableHead>
          <TableHead className="w-[120px]">Status</TableHead>
          <TableHead className="w-[70px]">Priority</TableHead>
          <TableHead className="w-[80px]">Deps</TableHead>
          <TableHead>Owner</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {groups.map((group) => {
          const groupKey = group.epic?.id ?? "__standalone__";
          const isCollapsed = collapsed.has(groupKey);
          const label = group.epic?.title ?? "Standalone tasks";
          return (
            <GroupBlock
              key={groupKey}
              groupKey={groupKey}
              label={label}
              group={group}
              isCollapsed={isCollapsed}
              onToggle={() => toggle(groupKey)}
              rowPad={rowPad}
              projectId={projectId}
              onSelectBead={onSelectBead}
            />
          );
        })}
        {groups.length === 0 && (
          <TableRow className="hover:bg-transparent">
            <TableCell colSpan={5} className="py-10 text-center text-zinc-500">
              No beads match the current filters.
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  );
}

function GroupBlock({
  groupKey,
  label,
  group,
  isCollapsed,
  onToggle,
  rowPad,
  projectId,
  onSelectBead,
}: {
  groupKey: string;
  label: string;
  group: EpicGroup;
  isCollapsed: boolean;
  onToggle: () => void;
  rowPad: string;
  projectId: string | null;
  onSelectBead: (bead: Bead) => void;
}) {
  return (
    <>
      {/* Epic group header row */}
      <TableRow className="border-zinc-800 bg-zinc-900/40 hover:bg-zinc-900/60">
        <TableCell colSpan={5} className="py-2">
          <div className="flex items-center gap-2">
            <button
              onClick={onToggle}
              className="rounded p-0.5 text-zinc-400 hover:text-zinc-100"
              aria-label={isCollapsed ? "Expand epic" : "Collapse epic"}
            >
              <ChevronRight
                className={cn(
                  "size-4 transition-transform",
                  !isCollapsed && "rotate-90"
                )}
              />
            </button>
            {group.epic ? (
              <Layers className="size-3.5 flex-shrink-0 text-purple-400" />
            ) : (
              <Circle className="size-3 flex-shrink-0 text-zinc-500" />
            )}
            <span className="truncate font-medium text-zinc-100">{label}</span>
            {group.epic && (
              <span className="flex-shrink-0 font-mono text-xs text-zinc-600">
                {formatBeadId(group.epic.id)}
              </span>
            )}
            <div className="ml-auto flex items-center gap-3">
              {group.blocked > 0 && (
                <span className="flex items-center gap-1 text-xs text-red-400">
                  <AlertTriangle className="size-3" /> {group.blocked}
                </span>
              )}
              <span className="text-xs text-zinc-400">
                {group.completed}/{group.children.length}
              </span>
              <Progress value={group.pct} className="h-1.5 w-24" />
              {group.epic && projectId && (
                <Link
                  href={`/epic?id=${projectId}&epic=${encodeURIComponent(
                    group.epic.id
                  )}`}
                  className="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200"
                  aria-label="Open full epic view"
                  title="Open full epic view"
                >
                  <Maximize2 className="size-3.5" />
                </Link>
              )}
            </div>
          </div>
        </TableCell>
      </TableRow>

      {/* Child rows */}
      {!isCollapsed &&
        group.children.map((bead) => {
          const blocked = isBlocked(bead);
          return (
            <TableRow
              key={bead.id}
              className="cursor-pointer"
              onClick={() => onSelectBead(bead)}
            >
              <TableCell className={rowPad}>
                <div className="flex items-center gap-2 pl-6">
                  <Circle
                    className={cn(
                      "size-2 flex-shrink-0 fill-current",
                      statusDotColor(bead.status)
                    )}
                    aria-hidden="true"
                  />
                  <span className="truncate text-zinc-200">{bead.title}</span>
                  <span className="ml-1 flex-shrink-0 font-mono text-xs text-zinc-600">
                    {formatBeadId(bead.id)}
                  </span>
                </div>
              </TableCell>
              <TableCell className={rowPad}>
                <span className={cn("text-xs", statusTextColor(bead.status))}>
                  {statusLabel(bead.status)}
                </span>
              </TableCell>
              <TableCell className={rowPad}>
                <span className={cn("text-xs", priorityColor(bead.priority))}>
                  {priorityLabel(bead.priority)}
                </span>
              </TableCell>
              <TableCell className={rowPad}>
                {blocked ? (
                  <span className="flex items-center gap-1 text-xs text-red-400">
                    <AlertTriangle className="size-3" />
                    {(bead.deps ?? []).length}
                  </span>
                ) : (
                  <span className="text-xs text-zinc-600">—</span>
                )}
              </TableCell>
              <TableCell className={rowPad}>
                <span className="truncate text-xs text-zinc-500">
                  {bead.owner?.split("@")[0] ?? "—"}
                </span>
              </TableCell>
            </TableRow>
          );
        })}
    </>
  );
}
