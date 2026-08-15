import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ImportProgress } from "../types";

const importStages: Array<{ phase: ImportProgress["phase"]; label: string }> = [
  { phase: "scanning", label: "Scan" },
  { phase: "hashing", label: "Verify" },
  { phase: "indexing", label: "Index" },
  { phase: "finalizing", label: "Shelf" },
];

export function ArchiveEmptyMark({ icon: Icon }: { icon: LucideIcon }) {
  return (
    <div className="quiet-empty-ready relative mb-4 h-14 w-[72px]" aria-hidden="true">
      <div className="absolute inset-x-1 top-0 h-12 rounded-md border bg-muted/10">
        <span className="absolute top-2 left-2 h-px w-3 bg-border" />
        <span className="absolute top-2 right-2 h-px w-3 bg-border" />
        <span className="absolute inset-0 grid place-items-center">
          <Icon className="size-5 text-muted-foreground" strokeWidth={1.5} />
        </span>
        <span className="absolute right-2 bottom-2 left-2 h-px bg-border" />
      </div>
      <span className="absolute bottom-0 left-1/2 h-1.5 w-7 -translate-x-1/2 rounded-t-sm border-x border-t border-primary/55 bg-primary/10" />
    </div>
  );
}

export function ImportStageRail({ phase }: { phase: ImportProgress["phase"] | null }) {
  const currentIndex = phase === "complete"
    ? importStages.length
    : importStages.findIndex((stage) => stage.phase === phase);

  return (
    <ol className="mb-3 grid grid-cols-4 gap-1.5" aria-hidden="true">
      {importStages.map((stage, index) => {
        const complete = currentIndex > index;
        const current = currentIndex === index;
        return (
          <li key={stage.phase} className="min-w-0">
            <span className="mb-1.5 block h-0.5 overflow-hidden rounded-full bg-border/80">
              <span className={cn(
                "block size-full origin-left rounded-full",
                complete && "bg-primary/55",
                current && "quiet-stage-settle bg-primary",
              )} />
            </span>
            <span className={cn(
              "block truncate text-[11px]",
              current ? "text-foreground" : complete ? "text-muted-foreground" : "text-muted-foreground/55",
            )}>{stage.label}</span>
          </li>
        );
      })}
    </ol>
  );
}
