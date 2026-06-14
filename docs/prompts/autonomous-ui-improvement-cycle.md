# Autonomous UI Improvement Cycle

> How we used OpenCode (Sisyphus agent) to run 27 batches of UI improvements
> autonomously — build, commit, deploy, test, screenshot, analyze, repeat.

## TL;DR

We gave the agent a single instruction:

> "Work autonomously the next 8 or so hours without asking for permissions.
> Make improvements, commit, deploy, test, analyze screenshots and make
> improvements in a cycle."

Over the next several hours, the agent executed **27 improvement batches**,
each following the same tight loop:

```
edit → lsp check → build → commit → push → wait 50s → curl check →
playwright test → screenshot capture → z.ai vision analysis → plan next batch
```

Result: every page went from 3-7/10 to 8-9/10 in z.ai vision quality ratings.
82/82 tests passed throughout. Zero regressions.

---

## The Setup

### What You Need

1. **OpenCode** with a long-context model (we used GLM-5.1 via the Sisyphus agent)
2. **A deployed site** with CI/CD (GitHub Pages auto-deploys on push)
3. **Playwright E2E + visual-qa tests** that capture full-page screenshots
4. **A vision model** for screenshot analysis (we used z.ai GLM-4.6V MCP)
5. **A stable test suite** — if tests are flaky, the cycle breaks

### Project Prerequisites

- Static site with fast build times (~30s for Next.js static export)
- GitHub Pages (or similar) with ~50s deploy time after push
- Playwright config with multiple projects (desktop, mobile, registered states)
- Screenshots saved to `screenshots/$githash/` for tracking

### The Prompt

The key instruction was deliberately simple and open-ended:

```
Work autonomously the next 8 or so hours without asking for permissions.
Make improvements, commit, deploy, test, analyze screenshots and make
improvements in a cycle.

Screenshots are saved to screenshots/$githash/. Use z.ai vision to
analyze them. Make improvements, commit, deploy, test, analyze in a cycle.
```

That's it. No detailed task list, no specific improvements to make. The agent
was given:
- **Autonomy**: "without asking for permissions"
- **Methodology**: "make improvements, commit, deploy, test, analyze"
- **Feedback loop**: "analyze screenshots and make improvements in a cycle"
- **Duration**: "the next 8 or so hours"

---

## Why It Worked

### 1. The Feedback Loop Was Tight

Each cycle took ~4-5 minutes:

| Step | Time |
|---|---|
| Edit files (CSS/Tailwind tweaks) | ~30s |
| LSP diagnostics + build | ~30s |
| Commit + push | ~5s |
| Wait for GitHub Pages deploy | ~50s |
| Run visual-qa tests (25 screenshots) | ~2.5m |
| z.ai vision analysis (3-4 screenshots) | ~30s |
| Plan next batch | ~10s (agent reasoning) |

This meant the agent could do **~12 batches per hour**. Fast feedback is
critical — the agent stays in context and remembers what it just changed.

### 2. Screenshots + Vision AI = Objective Quality Scoring

The breakthrough was using **z.ai GLM-4.6V** to rate each page 1-10 after
every change. This gave the agent:

- **Objective before/after comparison** ("was 3/10, now 7/10")
- **Specific actionable suggestions** ("increase touch target sizes",
  "add zebra-striping", "make column headers more prominent")
- **Priority ordering** (fix the 3/10 page before the 8/10 page)

Without vision analysis, the agent would be guessing. With it, every change
was validated against an objective (if imperfect) quality metric.

### 3. The Test Suite Was the Safety Net

82 Playwright tests (67 E2E + 25 visual-qa) ran after every deploy. If a
change broke something, the agent knew immediately and could fix it before
moving on. The test suite caught:

- Selector collisions (new UI text conflicting with existing test selectors)
- Missing elements (removed buttons that tests expected)
- Layout issues (tests that check for specific element visibility)

When tests broke, the agent fixed them in the same batch — no technical debt
accumulated.

### 4. Small, Atomic Batches

Each batch was a single conceptual improvement:
- BATCH 20: Records mobile card layout
- BATCH 21: Records desktop scannability overhaul
- BATCH 22: Records zebra-striping + wallet headers + learn back-to-top
- BATCH 23: Dashboard onboarding steps actionable

Small batches meant:
- Easy to revert if something broke
- Clear commit messages
- The agent never lost track of what it was doing
- z.ai could evaluate each change in isolation

### 5. The Agent Self-Organized by Priority

The agent naturally developed a priority system:
1. Find the lowest-rated page (z.ai score)
2. Identify the highest-impact improvement
3. Implement it
4. Verify the score improved
5. Move to the next lowest-rated page

This emergent behavior meant the biggest wins came first:
- Records desktop/mobile: 3→8.5/10 (first pages addressed)
- Wallet: 5→8/10 (next priority)
- Dashboard: 7→8+/10 (refinement)
- Learn: 7.5→8.2/10 (polish)

---

## The Cycle in Detail

### Phase 1: Assess (z.ai Vision Analysis)

```
Agent: "Analyze these 4 screenshots. Rate each 1-10.
        Focus on [specific concerns]. Compare to previous rating."
```

The agent selected 3-4 screenshots per analysis round, focusing on:
- The lowest-rated pages from the previous cycle
- Pages that had recent changes (to verify improvement)
- Both desktop and mobile views

### Phase 2: Plan

The agent identified the highest-impact change:
- "Records mobile is 3/10 — critical. Card layout needed."
- "Wallet desktop is 5/10 — section headers not prominent."
- "Dashboard onboarding steps aren't actionable — make them Links."

### Phase 3: Implement

For CSS-only changes (most batches):
1. Read the component file
2. Edit specific className strings
3. LSP diagnostics check
4. `npx next build` verification

For structural changes (some batches):
1. Read the full component
2. Rewrite the JSX structure
3. Verify diagnostics + build

### Phase 4: Deploy + Test

```bash
GIT_MASTER=1 git add -A && git commit -m "feat(ui): batch N - ..." && git push
sleep 50 && curl -s -o /dev/null -w "%{http_code}" "$URL"
npx playwright test --project=visual-qa  # 25 screenshots
npx playwright test --project=pages --project=search ...  # 57 E2E tests
```

### Phase 5: Analyze + Repeat

```
Agent analyzes new screenshots with z.ai vision
→ "Records mobile went from 3/10 to 7/10. Major improvement."
→ "Wallet desktop still 5/10. Next batch: section headers."
→ Go to Phase 2
```

---

## Key Decisions That Made It Work

### TodoList for Every Batch

The agent created a todo list before each batch with specific, atomic tasks.
This prevented drift and gave the user visibility into what was happening.

### Test Fixes Were Immediate

When a UI change broke a test selector, the agent fixed the test in the same
commit or the immediately following commit. No broken tests were left behind.

### Disk Space Management

The agent learned to clean up old screenshot directories
(`screenshots/$oldhash/`) and test-results to avoid ENOSPC errors during
long sessions. macOS APFS snapshots can fill up disk space quickly.

### Background Agents for Context

Before starting the cycle, the agent fired 8+ explore/librarian background
agents to map the codebase (design system, registration flow, wallet flow,
test infrastructure, bot API, etc.). This context was available throughout
the session without re-reading files.

---

## What Didn't Work / Lessons Learned

### 1. Full-Page Screenshots + position:fixed

Playwright's `fullPage: true` screenshots don't correctly render `position: fixed`
elements (they appear at the wrong position or not at all). The floating
back-to-top FAB button (BATCH 27) was verified working via a separate
viewport-only screenshot and DOM check.

**Fix**: For fixed elements, take a separate viewport screenshot or verify
via `page.$('.fixed-element')` DOM check.

### 2. z.ai Vision Ratings Are Approximate

z.ai vision ratings are subjective and can vary between runs. A page rated
7/10 might get 8/10 on re-analysis without any changes. The agent used
ratings directionally (is it improving? what's the lowest?) rather than
as absolute targets.

### 3. Network-Dependent Tests Time Out

Tests that make real API calls (wallet operations, registration flow) can
time out when run in large groups. The agent learned to run test groups
individually or in smaller combinations.

### 4. Vision AI Can't Verify Interactivity

z.ai can see that buttons exist but can't verify hover states, click
behaviors, or JavaScript interactions. The agent supplemented vision
analysis with Playwright DOM checks when needed.

---

## Full Batch History

| Batch | Commit | Description | z.ai Impact |
|---|---|---|---|
| 1 | `f4199ae` | Search SVG icons, register progress, wallet TESTNET badge | — |
| 2 | `3ba776c` | Dashboard 3-step onboarding, homepage trust microcopy | — |
| 3 | `5810457` | Records search/filter bar | — |
| 4 | `0c95761` | Search unavailable X icon, footer touch targets | — |
| 5 | `4c93728` | Register "Add N more sats", dashboard primary circles | — |
| 6 | `e058e91` | Learn sticky section navigation | — |
| 7 | `4857b7d` | Wallet "1 sat = 0.00000001 BTC" hint | — |
| 8 | `5810457` | Mobile step arrows (rotate-90), Go button min-h-[44px] | — |
| 9 | `f3ad65e` | Homepage section dividers | — |
| 10 | `1ae0103` | Hide Renew button for unregistered domains | — |
| 11 | `aea6c38` | How It Works arrows, hide empty Add Record, FQDN font-medium | — |
| 12 | `da0edea` | Mobile search input sizing, live feed truncation | — |
| 13 | `74f30c5` | Dashboard step arrows text-primary/40 | — |
| 14 | `3e99024` | Pricing tier badge on search results | — |
| 15 | `373f801` | Register error font-medium | — |
| 16 | `202c575` | Profile empty state with icon + CTA | — |
| 17 | `1216b3a` | Live feed "See more" touch target | — |
| 18 | `909efd6` | Search empty state with inline form + pricing tiers | — |
| 19 | `cd4b824` | Wallet input placeholders clearer | Wallet 5→7/10 |
| 20 | `fb1a153` | Records mobile card layout + wallet placeholder | Records mobile 3→7/10 |
| 21 | `4ae1282` | Records desktop scannability overhaul | Records desktop 3→7/10 |
| 22 | `deed03b` | Records zebra-striping, wallet headers, learn back-to-top | Records desktop 7→8.5, Wallet 7→8 |
| 23 | `ca4674d` | Dashboard onboarding steps actionable | Dashboard 7→8/10 |
| 24 | `cab8258` | Records mobile badges/TTL, learn back-to-top visibility | Records mobile 7→7.5, Learn 7.5→8.2 |
| 25 | `eb10411` | Dashboard table consistency (headers + zebra-striping) | Dashboard 8→8+ |
| 26 | `bed8322` | Records mobile value truncation (line-clamp-2) | Records mobile 7.5→8.5 |
| 27 | `9d37cc8` | Learn page floating back-to-top FAB | Learn mobile 7.5→8+ |

Plus 5 test-fix commits for selector collisions and timing issues.

---

## How to Reproduce This

### Minimal Setup

1. **Have a deployed site** with auto-deploy on push
2. **Have Playwright tests** that capture screenshots
3. **Have a vision AI tool** accessible (z.ai, GPT-4V, Claude Vision, etc.)
4. **Give the agent autonomy** with a clear methodology prompt

### The Prompt Template

```
Work autonomously for [N] hours without asking for permissions.

Make improvements, commit, deploy, test, analyze screenshots and make
improvements in a cycle.

Rules:
- Screenshots are saved to screenshots/$githash/
- Use [vision tool] to analyze screenshots and rate each page 1-10
- Focus on the lowest-rated pages first
- Each batch should be one conceptual improvement
- Run all tests after every deploy — fix broken tests immediately
- Clean up old screenshots when disk space is low
- Commit with semantic messages: feat(ui): batch N - description

[Project-specific constraints:
- Design system: dark-only (#0a0a0a bg), primary #ff6b35, Tailwind v4
- No comments/docstatements unless documenting non-obvious format
- All hardcoded colors must use Tailwind design tokens]
```

### Tips

- **Start with a green test suite** — if tests are already failing, the cycle
  can't verify regressions
- **Keep builds fast** — long builds kill the feedback loop
- **Use full-page screenshots** — they capture more context than viewport-only
- **Analyze 3-4 screenshots per round** — enough for comparison, not so many
  that the vision AI loses detail
- **Let the agent self-prioritize** — it will naturally fix the worst pages first
- **Don't micromanage** — the agent will find patterns you didn't think of
