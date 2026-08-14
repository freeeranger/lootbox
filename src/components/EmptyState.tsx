import type { LucideIcon } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState({ icon: Icon, title, description, action }: Props) {
  return (
    <div className="mx-auto flex max-w-xs flex-col items-center px-8 text-center">
      <Icon className="mb-3 size-5 text-muted-foreground" strokeWidth={1.5} />
      <h2 className="text-xs font-medium text-foreground">{title}</h2>
      <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">{description}</p>
      {action && (
        <Button type="button" variant="outline" size="sm" className="mt-3" onClick={action.onClick}>
          {action.label}
        </Button>
      )}
    </div>
  );
}
