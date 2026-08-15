import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FolderPlus } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { EmptyState } from "./EmptyState";
import { ImportStageRail } from "./QuietAcknowledgment";

describe("Quiet Acknowledgment", () => {
  it("keeps the empty archive action obvious", async () => {
    const user = userEvent.setup();
    const onClick = vi.fn();
    const { container } = render(
      <EmptyState
        icon={FolderPlus}
        title="No asset packs"
        description="Import folders to build a local catalog."
        action={{ label: "Import packs", onClick }}
        acknowledgment="archive"
      />,
    );

    expect(container.querySelector(".quiet-empty-ready")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Import packs" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("shows the truthful current import stage without a loop", () => {
    const { container } = render(<ImportStageRail phase="hashing" />);
    expect(screen.getByText("Verify")).toHaveClass("text-foreground");
    expect(screen.getByText("Scan")).toHaveClass("text-muted-foreground");
    expect(container.querySelectorAll(".quiet-stage-settle")).toHaveLength(1);
  });
});
