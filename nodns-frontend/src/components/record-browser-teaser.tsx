"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import type { ApiRecordsResponse } from "@/lib/types";
import { DatabaseIcon, GlobeIcon, ArrowRightIcon } from "lucide-react";
import { API_BASE } from "@/lib/constants";

export function RecordBrowserTeaser() {
  const [stats, setStats] = useState<{ total: number; domains: number } | null>(null);

  useEffect(() => {
    fetch(`${API_BASE}/api/records`)
      .then((r) => r.json())
      .then((data: ApiRecordsResponse) => {
        const recs = data.records ?? [];
        const npubs = new Set(recs.map((r) => r.npub));
        setStats({ total: recs.length, domains: npubs.size });
      })
      .catch(() => {});
  }, []);

  return (
    <section className="border-t border-border/40 px-6 py-16">
      <div className="mx-auto max-w-[960px]">
        <div className="rounded-xl bg-card ring-1 ring-foreground/10 p-8 md:p-10">
          <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-6">
            <div>
              <h2 className="text-xl font-bold mb-1">DNS Record Browser</h2>
              <p className="text-sm text-foreground/70 mb-4">
                Browse all DNS records verified from three independent sources
              </p>
              {stats && (
                <div className="flex items-center gap-4">
                  <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
                    <DatabaseIcon className="size-3.5" />
                    <span className="font-mono font-medium text-foreground">{stats.total}</span>{" "}
                    records
                  </div>
                  <div className="h-3 w-px bg-border" />
                  <div className="flex items-center gap-1.5 text-sm text-muted-foreground">
                    <GlobeIcon className="size-3.5" />
                    <span className="font-mono font-medium text-foreground">{stats.domains}</span>{" "}
                    domains
                  </div>
                </div>
              )}
            </div>
            <Link href="/records">
              <Button size="lg">
                Browse All Records
                <ArrowRightIcon className="size-4" />
              </Button>
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}
