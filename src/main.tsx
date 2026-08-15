import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { TooltipProvider } from "@/components/ui/tooltip";
import { installNativeShellBehavior } from "./nativeShell";
import "./styles.css";

if (typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window)) {
  (window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> }).__TAURI_INTERNALS__ = {
    convertFileSrc: (filePath: string, protocol = "asset") => `${protocol}://${filePath}`,
    plugins: {},
  };
}

installNativeShellBehavior();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: true,
    },
  },
});

class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Lootbox render failure", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <main className="dark grid size-full place-items-center bg-background p-8 text-foreground">
        <div className="w-full max-w-lg rounded-xl border border-destructive/30 bg-card p-5 shadow-xl">
          <p className="text-xs font-semibold text-destructive">Lootbox could not finish opening</p>
          <p className="mt-2 text-xs leading-relaxed text-muted-foreground">The interface hit an unexpected error. Reload the window, and copy the detail below if it happens again.</p>
          <pre className="select-text mt-4 max-h-40 overflow-auto rounded-lg border bg-background p-3 font-mono text-[11px] leading-relaxed text-muted-foreground">{this.state.error.message}</pre>
          <button type="button" className="mt-4 h-8 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground" onClick={() => window.location.reload()}>
            Reload Lootbox
          </button>
        </div>
      </main>
    );
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={450}>
        <AppErrorBoundary>
          <App />
        </AppErrorBoundary>
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>,
);
