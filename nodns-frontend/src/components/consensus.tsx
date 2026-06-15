"use client";

import { useState } from "react";
import consensusData from "../../../content/consensus.json";

type Status =
  | "external-reference"
  | "draft"
  | "proof-of-concept"
  | "thought-experiment"
  | "research-todo";

interface DualResolution {
  dns: string;
  nostr: string;
  conflict: string;
}

interface Example {
  name: string;
  description: string;
  url: string;
}

interface Reference {
  title: string;
  url: string;
  description?: string;
}

interface Model {
  id: string;
  name: string;
  tagline: string;
  status: Status;
  authority: string;
  ownership_model: string;
  censorship_resistance: string;
  token: boolean;
  pattern?: string;
  example?: string;
  url?: string;
  description: string;
  pros?: string[];
  cons?: string[];
  reference?: Reference;
  examples?: Example[];
  resolution?: string;
  dual_resolution?: DualResolution;
  philosophy?: string;
  open_questions?: string[];
}

interface ComparisonRow {
  columns: string[];
  rows: string[][];
}

interface Closing {
  title: string;
  body: string;
  no_token: string;
  research: string;
  nothing_is_production: string;
}

interface Principle {
  id: string;
  statement: string;
  payments: {
    anti_spam: string;
    namespace_lease: string;
    never: string;
  };
}

interface ConsensusData {
  title: string;
  subtitle: string;
  intro: string;
  principle: Principle;
  models: Model[];
  comparison_table: ComparisonRow;
  closing: Closing;
}

const data = consensusData as ConsensusData;

const STATUS_STYLES: Record<Status, { bg: string; text: string; label: string }> = {
  "external-reference": { bg: "bg-secondary", text: "text-muted-foreground", label: "External" },
  draft: { bg: "bg-blue-950/30", text: "text-blue-400", label: "Draft" },
  "proof-of-concept": { bg: "bg-emerald-950/30", text: "text-emerald-400", label: "Proof of Concept" },
  "thought-experiment": { bg: "bg-yellow-950/30", text: "text-yellow-400", label: "Thought Experiment" },
  "research-todo": { bg: "bg-purple-950/30", text: "text-purple-400", label: "Research TODO" },
};

function StatusBadge({ status }: { status: Status }) {
  const s = STATUS_STYLES[status];
  return (
    <span
      className={`inline-block rounded-full px-2.5 py-0.5 text-[0.7rem] font-semibold uppercase tracking-wider whitespace-nowrap ${s.bg} ${s.text}`}
    >
      {s.label}
    </span>
  );
}

function ModelCard({ model }: { model: Model }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div id={model.id} className="mb-5 rounded-xl border border-border bg-card p-6">
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <StatusBadge status={model.status} />
        <h3 className="text-lg font-semibold">{model.name}</h3>
        <span className="text-sm text-muted-foreground">{model.tagline}</span>
      </div>

      <p className="mb-3 text-foreground/70">{model.description}</p>

      <p className="mb-1 text-sm">
        <strong>Authority:</strong>{" "}
        <span className="text-muted-foreground">{model.authority}</span>
      </p>

      {model.pattern && (
        <p className="mb-1 text-sm">
          <strong>Pattern:</strong>{" "}
          <code className="font-mono text-[0.85rem] text-primary">{model.pattern}</code>
          {model.example && (
            <>
              {" "}
              →{" "}
              <code className="font-mono text-[0.85rem] text-primary">
                {model.example}
              </code>
            </>
          )}
        </p>
      )}

      {model.reference && (
        <p className="mb-1 text-sm">
          <strong>Ref:</strong>{" "}
          <a
            href={model.reference.url}
            className="text-primary hover:underline"
            target="_blank"
            rel="noopener noreferrer"
          >
            {model.reference.title}
          </a>
        </p>
      )}

      {model.examples && (
        <div className="my-3">
          {model.examples.map((ex) => (
            <p key={ex.name} className="text-sm">
              <a
                href={ex.url}
                className="text-primary hover:underline"
                target="_blank"
                rel="noopener noreferrer"
              >
                {ex.name}
              </a>{" "}
              — <span className="text-muted-foreground">{ex.description}</span>
            </p>
          ))}
        </div>
      )}

      {model.url && !model.reference && (
        <p className="text-sm">
          <a
            href={model.url}
            className="text-primary hover:underline"
            target="_blank"
            rel="noopener noreferrer"
          >
            Learn more →
          </a>
        </p>
      )}

      {model.dual_resolution && (
        <div className="my-4 grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="rounded-lg border border-border bg-background p-4">
            <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              DNS
            </h4>
            <code className="text-[0.8rem] text-primary">{model.dual_resolution.dns}</code>
          </div>
          <div className="rounded-lg border border-border bg-background p-4">
            <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              Nostr
            </h4>
            <code className="text-[0.8rem] text-primary">{model.dual_resolution.nostr}</code>
          </div>
          <div className="rounded-lg border border-border bg-background p-4">
            <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
              Conflict
            </h4>
            <code className="text-[0.8rem] text-primary">{model.dual_resolution.conflict}</code>
          </div>
        </div>
      )}

      {model.resolution && !model.dual_resolution && (
        <p className="mt-3 text-sm">
          <strong>Resolution:</strong>{" "}
          <span className="text-muted-foreground">{model.resolution}</span>
        </p>
      )}

      <button
        onClick={() => setExpanded(!expanded)}
        className="mt-3 text-sm font-medium text-primary hover:underline"
      >
        {expanded ? "▾ Hide details" : "▸ Show details"}
      </button>

      {expanded && (
        <div className="mt-4 space-y-4 border-t border-border pt-4">
          {model.philosophy && (
            <div>
              <h4 className="mb-2 text-sm font-semibold">Philosophy</h4>
              <p className="italic text-foreground/70">{model.philosophy}</p>
            </div>
          )}

          {(model.pros || model.cons) && (
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              {model.pros && (
                <div>
                  <h4 className="mb-2 text-sm font-semibold text-emerald-400">
                    ✓ Pros
                  </h4>
                  <ul className="list-disc pl-5 text-sm text-foreground/70">
                    {model.pros.map((p, i) => (
                      <li key={i}>{p}</li>
                    ))}
                  </ul>
                </div>
              )}
              {model.cons && (
                <div>
                  <h4 className="mb-2 text-sm font-semibold text-red-400">
                    ✗ Cons
                  </h4>
                  <ul className="list-disc pl-5 text-sm text-foreground/70">
                    {model.cons.map((c, i) => (
                      <li key={i}>{c}</li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
          )}

          {model.open_questions && (
            <div>
              <h4 className="mb-2 text-sm font-semibold text-yellow-400">
                Open Questions
              </h4>
              <ul className="list-disc pl-5 text-sm text-foreground/70">
                {model.open_questions.map((q, i) => (
                  <li key={i}>{q}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function Consensus() {
  return (
    <div className="px-6 py-8">
      <div className="mx-auto max-w-[960px]">
        <p className="mb-4 text-lg text-foreground/70">{data.subtitle}</p>
        <p className="mb-8 text-foreground/70">{data.intro}</p>

        {/* No-token principle */}
        <div className="mb-12 rounded-xl border border-emerald-800/30 bg-emerald-950/20 p-6">
          <h3 className="mb-3 text-lg font-semibold text-emerald-400">
            ⛔ No Token. Ever.
          </h3>
          <p className="mb-4 text-foreground/70">{data.principle.statement}</p>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div>
              <h4 className="mb-1 text-sm font-semibold text-emerald-400">Anti-spam</h4>
              <p className="text-sm text-foreground/70">{data.principle.payments.anti_spam}</p>
            </div>
            <div>
              <h4 className="mb-1 text-sm font-semibold text-primary">Namespace lease</h4>
              <p className="text-sm text-foreground/70">{data.principle.payments.namespace_lease}</p>
            </div>
          </div>
          <p className="mt-3 text-sm text-red-400">{data.principle.payments.never}</p>
        </div>

        {/* Models */}
        <h2 className="mb-6 text-[1.75rem] font-bold tracking-tight">
          Consensus Models
        </h2>
        {data.models.map((model) => (
          <ModelCard key={model.id} model={model} />
        ))}

        {/* Comparison table */}
        <h2 className="mb-4 mt-12 text-[1.75rem] font-bold tracking-tight">
          At a Glance
        </h2>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr>
                {data.comparison_table.columns.map((col) => (
                  <th
                    key={col}
                    className="px-3 py-2.5 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground"
                  >
                    {col}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.comparison_table.rows.map((row, i) => (
                <tr key={i} className="border-b border-border">
                  {row.map((cell, j) => (
                    <td
                      key={j}
                      className={`px-3 py-2.5 ${
                        j === 0 ? "font-medium text-foreground" : "text-muted-foreground"
                      }`}
                    >
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Closing */}
        <div className="mt-12 rounded-xl border border-border bg-card p-8">
          <h2 className="mb-4 text-[1.75rem] font-bold tracking-tight">
            {data.closing.title}
          </h2>
          <p className="mb-3 text-foreground/70">{data.closing.body}</p>
          <p className="mb-2 text-sm">
            <strong>No token:</strong>{" "}
            <span className="text-muted-foreground">{data.closing.no_token}</span>
          </p>
          <p className="mb-2 text-sm">
            <strong>Research:</strong>{" "}
            <span className="text-muted-foreground">{data.closing.research}</span>
          </p>
          <p className="text-sm text-yellow-400">{data.closing.nothing_is_production}</p>
        </div>
      </div>
    </div>
  );
}
