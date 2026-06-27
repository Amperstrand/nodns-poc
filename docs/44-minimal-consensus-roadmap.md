# 44 — Minimal Consensus Standard Roadmap

> **Status**: DRAFT. This is a talk prep and direction document, not a protocol spec.

## Purpose

This doc collects the decisions we want to surface in the talk and the smallest useful standard we can agree on before moving into a fuller v2.

The goal is not perfection. The goal is a minimal, understandable consensus that people can implement and discuss.

## Recommendation

### 1) Name classes

- **`$npub.tld`**: always true by cryptography.
- **`$string.tld`**: requires namespace-owner opt-in and a pledge to mirror for a fee.
- This keeps the trust model clear: npub is identity, string is a lease.

### 2) Protocol version

- Stay on **kind 11111** for now.
- Treat **31111** as the later migration step once the rest of the stack is ready.
- The migration can be framed as the "we are live" moment.

### 3) Custom-name registration

- Keep it simple: **bid → accept bid**.
- The namespace owner signals acceptance.
- This is closer to a fee market than a full auction system.

### 4) Edit semantics

- For normal DNS, **edit = overwrite**.
- No new mental model needed; reuse the DNS expectation.

### 5) Viewer/tooling

- Build modular views, not one giant interface.
- One **debug/log view** that shows everything and supports filtering.
- One **personal view** that only shows records/events for your npub.

### 6) Docs framing

- Separate docs into **what we are shipping now** and **ideas for later**.
- Leave advanced auction/takeover/PoW/multi-operator work as future research.

## Decision Table

| Topic | Minimal choice | Why |
|---|---|---|
| `$npub.tld` | Always true | No ambiguity: the key is the name |
| `$string.tld` | Owner opt-in + pledge | Keeps the operator as the lease holder |
| Wire format | Keep 11111 for now | Lowers churn while the rest settles |
| Future migration | 31111 when ready | Clear “we’re live” transition |
| Custom names | Bid then accept | Minimal and understandable |
| Editing | Overwrite current record | Matches normal DNS expectations |
| Viewers | Modular debug + personal views | Supports both analysis and everyday use |
| Docs | Now vs future split | Makes the project easier to explain |

## Talk Outline

### Slide 1 — The problem

NoDNS is trying to turn Nostr events into a practical naming system without pretending every name class has the same trust model.

### Slide 2 — Two name classes

Explain `$npub.tld` vs `$string.tld`.

### Slide 3 — Minimal standard

The minimum useful standard is: simple wire format, simple rules, simple viewers.

### Slide 4 — What is decided now

- npub names are always true
- string names need opt-in
- edits overwrite
- keep 11111 for now

### Slide 5 — What stays open

- 31111 migration
- bids vs auctions
- future anti-spam and takeover mechanics

### Slide 6 — Tooling

Show the need for a debug viewer, personal viewer, CLI generation, and browser publishing.

### Slide 7 — Why this matters

A minimal standard makes it easier for others to implement, critique, and extend NoDNS.

### Slide 8 — Ask

Ask the room which parts should become standard now and which should remain future ideas.

## Docs Restructuring Plan

### Keep as "Now"

These docs describe the current minimal PoC or directly support it:

- `11-protocol-experimental-draft.md`
- `20-design-philosophy.md`
- `21-name-classes.md`
- `22-pricing-and-payments.md`
- `23-lease-and-renewal.md`
- `24-race-conditions.md`
- `27-implementation-plan.md`
- `33-faq.md`
- `34-backwards-compatible-apis.md`
- `35-bot-deployment-runbook.md`

### Keep as "Future"

These docs are good ideas, but they are not the minimal PoC:

- `36-anti-spam-research.md`
- `37-cv-trust-model.md`
- `38-nostr-alignment-research.md`
- `39-protocol-v2-design.md`
- `40-bridge-agent-architecture.md`
- `41-nostr-over-dns-experiment.md`
- `42-kind-31111-migration.md`
- `43-payment-escrow-model.md`

### Suggested structure after the talk

1. A short **minimal standard** doc.
2. A short **current PoC** doc.
3. A separate **future ideas** index.
4. Keep the long-form reasoning docs for traceability.

## Next Step

Use this outline to drive the talk, then turn the accepted decisions into the smallest possible implementation milestone.
