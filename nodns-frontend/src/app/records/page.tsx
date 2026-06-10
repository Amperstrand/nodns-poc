"use client";

import { Suspense } from "react";
import { SiteHeader } from "@/components/site-header";
import { SiteFooter } from "@/components/site-footer";
import { ErrorBoundary } from "@/components/error-boundary";
import { RecordBrowser } from "@/components/record-browser";

export default function RecordsPage() {
  return (
    <ErrorBoundary>
      <SiteHeader />
      <main className="px-6 pb-16">
        <Suspense
          fallback={
            <div className="mx-auto max-w-[960px] py-20 text-center text-muted-foreground animate-pulse">
              Loading records...
            </div>
          }
        >
          <RecordBrowser />
        </Suspense>
      </main>
      <SiteFooter />
    </ErrorBoundary>
  );
}
