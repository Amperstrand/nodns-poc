"use client";

import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("[ErrorBoundary] Render crash:", error, info.componentStack);
  }

  private handleReload = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className="flex min-h-screen items-center justify-center bg-background px-6">
           <div className="max-w-md rounded-xl border border-red-500/25 bg-card p-8 text-center">
             <div className="mb-4 text-4xl">&#9888;&#65039;</div>
             <h1 className="mb-2 text-xl font-bold text-foreground">
               Something went wrong
             </h1>
             <p className="mb-6 text-sm text-muted-foreground">
               An unexpected error occurred while rendering this page. The error
               has been logged to the console.
             </p>
             {this.state.error && (
               <pre className="mb-6 max-h-[120px] overflow-auto rounded-lg border border-border bg-background p-3 text-left font-mono text-xs text-destructive">
                 {this.state.error.message}
               </pre>
             )}
             <button
               onClick={this.handleReload}
               className="rounded-lg bg-primary px-6 py-2.5 text-sm font-semibold text-primary-foreground transition-opacity hover:opacity-90"
             >
              Reload
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
