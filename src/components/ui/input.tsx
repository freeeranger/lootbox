import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"
import { ClipboardPaste, Copy, Redo2, Scissors, TextSelect, Undo2 } from "lucide-react"

import { cn } from "@/lib/utils"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"

function Input({ className, type, ref, onContextMenu, ...props }: React.ComponentProps<"input">) {
  const inputRef = React.useRef<HTMLInputElement | null>(null)
  const [selection, setSelection] = React.useState({ start: 0, end: 0 })
  const editable = !props.disabled && !props.readOnly
  const hasSelection = selection.end > selection.start

  function assignRef(node: HTMLInputElement | null) {
    inputRef.current = node
    if (typeof ref === "function") ref(node)
    else if (ref) ref.current = node
  }

  function restoreSelection() {
    const input = inputRef.current
    if (!input) return null
    input.focus()
    input.setSelectionRange(selection.start, selection.end)
    return input
  }

  function runEditingCommand(command: "undo" | "redo" | "copy" | "cut") {
    if (!restoreSelection()) return
    document.execCommand(command)
  }

  async function paste() {
    const input = restoreSelection()
    if (!input) return
    try {
      const text = await navigator.clipboard.readText()
      input.setRangeText(text, selection.start, selection.end, "end")
      input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertFromPaste", data: text }))
    } catch {
      document.execCommand("paste")
    }
  }

  const input = (
    <InputPrimitive
      {...props}
      ref={assignRef}
      type={type}
      onContextMenu={(event) => {
        setSelection({ start: event.currentTarget.selectionStart ?? 0, end: event.currentTarget.selectionEnd ?? 0 })
        onContextMenu?.(event)
      }}
      autoComplete={props.autoComplete ?? "off"}
      autoCorrect={props.autoCorrect ?? "off"}
      autoCapitalize={props.autoCapitalize ?? "off"}
      spellCheck={props.spellCheck ?? false}
      data-slot="input"
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
        <ContextMenuItem disabled={!editable} onClick={() => runEditingCommand("undo")}><Undo2 /> Undo <span className="ml-auto text-[11px] text-muted-foreground">Ctrl Z</span></ContextMenuItem>
        <ContextMenuItem disabled={!editable} onClick={() => runEditingCommand("redo")}><Redo2 /> Redo <span className="ml-auto text-[11px] text-muted-foreground">Ctrl Shift Z</span></ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!editable || !hasSelection} onClick={() => runEditingCommand("cut")}><Scissors /> Cut <span className="ml-auto text-[11px] text-muted-foreground">Ctrl X</span></ContextMenuItem>
        <ContextMenuItem disabled={!hasSelection} onClick={() => runEditingCommand("copy")}><Copy /> Copy <span className="ml-auto text-[11px] text-muted-foreground">Ctrl C</span></ContextMenuItem>
        <ContextMenuItem disabled={!editable} onClick={() => void paste()}><ClipboardPaste /> Paste <span className="ml-auto text-[11px] text-muted-foreground">Ctrl V</span></ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={() => { const target = inputRef.current; target?.focus(); target?.select(); }}><TextSelect /> Select all <span className="ml-auto text-[11px] text-muted-foreground">Ctrl A</span></ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

export { Input }
