"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";

const STEPS = [
  {
    title: "Mine PoW",
    snippet: [
      "Mining NIP-13 proof-of-work...",
      "difficulty: 20 bits",
      "nonce: searching...",
      "✓ Valid nonce found",
    ],
  },
  {
    title: "Publish Event",
    snippet: [
      "kind: 11111",
      'tags: [["record", "A", "@", "185.18.221.10"]]',
    ],
  },
  {
    title: "Bot Validates",
    snippet: [
      "✓ Event signature valid",
      "✓ PoW difficulty met (20 bits)",
      "✓ Authority confirmed",
    ],
  },
  {
    title: "DDNS Update",
    snippet: [
      "UPDATE npub190q...nodns.shop",
      "  IN A 185.18.221.10",
      "  TTL 3600",
      "→ TSIG signed ✓",
    ],
  },
  {
    title: "DNS Resolves",
    snippet: [
      "dig npub190q...nodns.shop A",
      "→ 185.18.221.10",
      "→ 3 second propagation",
    ],
  },
];

const STEP_MS = 3000;
const SUCCESS_MS = 2000;

export function PublishDemo() {
  const [step, setStep] = useState(0);
  const [progressKey, setProgressKey] = useState(0);

  // step: 0–3 = active pipeline stage, 4 = success flash
  useEffect(() => {
    const ms = step === 4 ? SUCCESS_MS : STEP_MS;
    const timer = setTimeout(() => {
      setStep((s) => (s >= 5 ? 0 : s + 1));
      setProgressKey((k) => k + 1);
    }, ms);
    return () => clearTimeout(timer);
  }, [step]);

  const isSuccess = step === 5;
  const activeIdx = isSuccess ? 4 : step;

  return (
    <section className="px-6 py-16">
      <div className="mx-auto max-w-[560px] rounded-xl bg-card p-6 ring-1 ring-foreground/10 sm:p-8">
        {/* Header */}
        <div className="mb-6 flex items-center justify-between">
          <h3 className="text-lg font-semibold text-foreground">
            Publish Pipeline
          </h3>
          {isSuccess ? (
            <Badge variant="default">Record live!</Badge>
          ) : (
            <Badge variant="outline">{step + 1} / 5</Badge>
          )}
        </div>

        {/* Vertical Pipeline */}
        <div className="relative flex flex-col">
          {STEPS.map((s, i) => {
            const done = i < activeIdx || isSuccess;
            const active = i === activeIdx && !isSuccess;

            return (
              <div key={i} className="relative">
                {/* Connector line to previous step */}
                {i > 0 && (
                  <div className="absolute left-[11px] top-0 h-3 w-0.5 -translate-y-full bg-border">
                    <div
                      className="w-full bg-primary transition-all duration-700"
                      style={{ height: done || active ? "100%" : "0%" }}
                    />
                  </div>
                )}
                <div className="flex items-center gap-3 py-2.5">
                  {/* Indicator */}
                  <div
                    className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-xs font-bold transition-all duration-500 ${
                      done
                        ? "border-primary bg-primary text-primary-foreground"
                        : active
                          ? "border-primary text-primary ring-2 ring-primary/30"
                          : "border-border text-muted-foreground"
                    }`}
                  >
                    {done ? "✓" : i + 1}
                  </div>
                  {/* Label */}
                  <span
                    className={`text-sm font-medium transition-colors duration-300 ${
                      done
                        ? "text-foreground"
                        : active
                          ? "text-foreground"
                          : "text-muted-foreground/50"
                    }`}
                  >
                    {s.title}
                  </span>
                </div>
              </div>
            );
          })}
        </div>

        {/* Code Panel */}
        <div className="relative mt-5 overflow-hidden rounded-lg bg-background p-4 ring-1 ring-foreground/5">
          <pre className="overflow-x-auto text-xs leading-relaxed text-foreground/70">
            <code>
              {isSuccess
                ? "✓ Record live! npub190q...nodns.shop → 185.18.221.10"
                : STEPS[activeIdx].snippet.join("\n")}
            </code>
          </pre>
          {/* Auto-advance progress bar */}
          {!isSuccess && (
            <div
              key={progressKey}
              className="absolute inset-x-0 bottom-0 h-[2px] origin-left animate-[progress-fill_3s_linear_forwards] bg-primary/40"
            />
          )}
        </div>

        {/* CTA */}
        <div className="mt-6 text-center">
          <Link
            href="/register"
            className="inline-flex h-9 items-center gap-1.5 rounded-lg bg-primary px-5 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/80 transition-colors"
          >
            Try It Yourself
          </Link>
        </div>
      </div>

      {/* Keyframe for progress bar — scoped via style tag */}
      <style>{`
        @keyframes progress-fill {
          from { transform: scaleX(0); }
          to   { transform: scaleX(1); }
        }
      `}</style>
    </section>
  );
}
