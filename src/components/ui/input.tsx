import * as React from "react"
import { ClipboardPaste, Copy, Redo2, Scissors, TextSelect, Undo2 } from "lucide-react"

import { cn } from "@/lib/utils"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"

import { formatForDisplay } from "@tanstack/react-hotkeys"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  const inputRef = React.useRef<HTMLInputElement | null>(null)
  const [hasSelection, setHasSelection] = React.useState(false)

  const updateSelectionState = React.useCallback(() => {
    const input = inputRef.current
    if (!input) return
    const start = input.selectionStart ?? 0
    const end = input.selectionEnd ?? 0
    setHasSelection(end > start)
  }, [])

  const runEditingCommand = (command: "undo" | "redo" | "cut" | "copy") => {
    const input = inputRef.current
    if (!input) return
    input.focus()
    document.execCommand(command)
    updateSelectionState()
  }

  const paste = async () => {
    const input = inputRef.current
    if (!input) return
    input.focus()
    if (navigator.clipboard?.readText) {
      try {
        const text = await navigator.clipboard.readText()
        const start = input.selectionStart ?? 0
        const end = input.selectionEnd ?? input.value.length
        input.setRangeText(text, start, end, "end")
        input.dispatchEvent(new Event("input", { bubbles: true }))
      } catch {
        document.execCommand("paste")
      }
    } else {
      document.execCommand("paste")
    }
    updateSelectionState()
  }

  const editable = !props.disabled && !props.readOnly

  const input = (
    <input
      type={type}
      data-slot="input"
      ref={inputRef}
      onSelect={updateSelectionState}
      onKeyUp={updateSelectionState}
      onMouseUp={updateSelectionState}
      {...props}
      className={cn(
        "h-8 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 py-1 text-base transition-colors outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 md:text-sm dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
        className
      )}
    />
  )

  return (
    <ContextMenu>
      <ContextMenuTrigger render={input} />
      <ContextMenuContent className="min-w-48">
        <ContextMenuItem disabled={!editable} onClick={() => runEditingCommand("undo")}><Undo2 /> Undo <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+Z")}</span></ContextMenuItem>
        <ContextMenuItem disabled={!editable} onClick={() => runEditingCommand("redo")}><Redo2 /> Redo <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+Shift+Z")}</span></ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!editable || !hasSelection} onClick={() => runEditingCommand("cut")}><Scissors /> Cut <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+X")}</span></ContextMenuItem>
        <ContextMenuItem disabled={!hasSelection} onClick={() => runEditingCommand("copy")}><Copy /> Copy <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+C")}</span></ContextMenuItem>
        <ContextMenuItem disabled={!editable} onClick={() => void paste()}><ClipboardPaste /> Paste <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+V")}</span></ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => { const target = inputRef.current; target?.focus(); target?.select(); }}><TextSelect /> Select all <span className="ml-auto text-[11px] text-muted-foreground">{formatForDisplay("Mod+A")}</span></ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

export { Input }
