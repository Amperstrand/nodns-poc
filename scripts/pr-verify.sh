#!/usr/bin/env bash
# pr-verify.sh — the LOCAL CI gate for nodns-poc.
# Fleet pattern: Amperstrand/.github workflow-templates/local-ci-gate.md
# (reference implementation: tollgate-module-basic-go scripts/pr-verify.sh).
#
# Legs:
#   hygiene      conflict-marker grep over the tracked tree
#   fmt          cargo fmt --all -- --check
#   clippy       cargo clippy --workspace --all-targets -- -D warnings
#   tests        cargo test --workspace
#
# NOT covered on purpose (say so, never imply otherwise): the Next.js frontend
# build, the TXT-registration script (ci/register-txt.mjs — talks to live DNS),
# deploy targets, and anything relay/nameserver-visible.
#
# Usage:
#   scripts/pr-verify.sh 12                 # PR mode: verify + comment
#   scripts/pr-verify.sh 12 --no-post       # verify, print table only
#   scripts/pr-verify.sh origin/main        # ref mode: verify + print
#   scripts/pr-verify.sh 12 --only clippy,tests
#
# No secrets are read or embedded; PR mode uses the invoking account's gh
# auth for view/comment only. PR mode pins the head SHA at start-of-run.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEGS=(hygiene fmt clippy tests)

usage() {
    echo "usage: $0 <PR-number | git-ref> [--no-post] [--only LEG[,LEG...]] [--remove-logs]" >&2
    echo "legs: ${LEGS[*]}" >&2
    exit 2
}
die() { echo "pr-verify: $*" >&2; exit 2; }

TARGET=""; NO_POST=0; REMOVE_LOGS=0; ONLY=""
while [ $# -gt 0 ]; do
    case "$1" in
        --no-post) NO_POST=1 ;;
        --remove-logs) REMOVE_LOGS=1 ;;
        --only) [ $# -ge 2 ] || die "--only needs a comma-separated leg list"; ONLY="$2"; shift ;;
        --only=*) ONLY="${1#--only=}" ;;
        -h|--help) usage ;;
        -*) die "unknown option: $1" ;;
        *) [ -z "$TARGET" ] || die "one target only (got '$TARGET' and '$1')"; TARGET="$1" ;;
    esac
    shift
done
[ -n "$TARGET" ] || usage

if [ -n "$ONLY" ]; then
    IFS=',' read -r -a wanted <<<"$ONLY"
    for w in "${wanted[@]}"; do
        ok=0; for l in "${LEGS[@]}"; do [ "$w" = "$l" ] && ok=1; done
        [ "$ok" = 1 ] || die "unknown leg '$w' (legs: ${LEGS[*]})"
    done
else
    wanted=("${LEGS[@]}")
fi
want_leg() { local L; for L in "${wanted[@]}"; do [ "$L" = "$1" ] && return 0; done; return 1; }

for c in git cargo; do command -v "$c" >/dev/null 2>&1 || die "$c is required but not on PATH"; done

SLUG="$(git -C "$ROOT" remote get-url origin 2>/dev/null | sed -E 's#.*github\.com[:/]##; s#\.git$##')"
[ -n "$SLUG" ] || die "cannot derive the GitHub slug from the origin remote"

PR_NUMBER=""; PR_TITLE=""; PR_STATE=""; HEAD_REF=""; BASE_SHA=""
if [[ "$TARGET" =~ ^[0-9]+$ ]]; then
    command -v gh >/dev/null 2>&1 || die "PR mode needs gh (ref: $SLUG)"
    PR_NUMBER="$TARGET"
    meta="$(gh pr view "$PR_NUMBER" -R "$SLUG" --json title,state,headRefName,headRefOid --jq='[.title,.state,.headRefName,.headRefOid]|@tsv' 2>/dev/null)" \
        || die "gh pr view $PR_NUMBER failed"
    IFS=$'\t' read -r PR_TITLE PR_STATE HEAD_REF HEAD_OID <<<"$meta"
    [ "$PR_STATE" = "OPEN" ] || die "PR #$PR_NUMBER is $PR_STATE — verify open PRs"
    git -C "$ROOT" fetch --quiet origin main 2>/dev/null || true
    BASE_SHA="$(git -C "$ROOT" merge-base "$HEAD_OID" origin/main 2>/dev/null || echo '?')"
    echo "== PR #$PR_NUMBER: $PR_TITLE"
    echo "== head $HEAD_REF @ ${HEAD_OID:0:10} (base ${BASE_SHA:0:10})"
else
    HEAD_REF="$TARGET"
    case "$TARGET" in
        origin/*)
            # Refresh the tracking ref from the remote BEFORE resolving —
            # resolving a stale local origin/main pins an outdated OID (#157).
            git -C "$ROOT" fetch --quiet origin \
                "+refs/heads/${TARGET#origin/}:refs/remotes/origin/${TARGET#origin/}" \
                2>/dev/null || true
            ;;
    esac
    if HEAD_OID="$(git -C "$ROOT" rev-parse --verify --quiet "$TARGET^{commit}")"; then
        echo "== ref $TARGET @ ${HEAD_OID:0:10} (resolved after remote refresh)"
    else
        git -C "$ROOT" fetch --quiet origin "$TARGET" || die "cannot fetch '$TARGET' from origin"
        HEAD_OID="$(git -C "$ROOT" rev-parse --verify FETCH_HEAD^{commit})" || die "FETCH_HEAD did not resolve to a commit"
    fi
    BASE_SHA="$(git -C "$ROOT" rev-parse origin/main 2>/dev/null || echo "?")"
    echo "== ref $TARGET @ ${HEAD_OID:0:10}"
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
LOGDIR="/tmp/nodnspoc-pr-verify/${TARGET//-/_}-$STAMP"
mkdir -p "$LOGDIR"
WT="$LOGDIR/worktree"
cleanup() {
    git -C "$ROOT" worktree remove --force "$WT" >/dev/null 2>&1 || true
    git -C "$ROOT" worktree prune >/dev/null 2>&1 || true
    if [ "$REMOVE_LOGS" = 1 ]; then rm -rf "$LOGDIR"; else
        rm -rf "$WT" 2>/dev/null || true
        echo "== logs kept in $LOGDIR"
    fi
}
trap cleanup EXIT

if [ -n "$PR_NUMBER" ]; then
    # Fork PRs: headRefName lives in the contributor's repo — the base origin
    # doesn't carry the branch. Fetch the pinned OID (GitHub serves exact SHAs
    # of PR heads), falling back to GitHub's refs/pull/N/head (#157).
    git -C "$ROOT" fetch --quiet origin "$HEAD_OID" 2>/dev/null \
        || git -C "$ROOT" fetch --quiet origin "refs/pull/${PR_NUMBER}/head" 2>/dev/null \
        || true
else
    git -C "$ROOT" fetch --quiet origin "$HEAD_REF" 2>/dev/null || true
fi
git -C "$ROOT" rev-parse --verify --quiet "${HEAD_OID}^{commit}" >/dev/null \
    || die "head commit $HEAD_OID is not local after fetch — rerun"
git -C "$ROOT" worktree add --quiet --detach "$WT" "$HEAD_OID" || die "worktree add failed for $HEAD_OID"
echo "== worktree: $WT"

# Hash the script that actually EXECUTES (the caller's copy), not the
# worktree's — when the PR changes this script the two differ (#157).
INVOKED="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"
    elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"   # macOS
    else return 1; fi
}
SCRIPT_ID="$(hash_file "$INVOKED" 2>/dev/null | cut -c1-12)"
[ -n "$SCRIPT_ID" ] || SCRIPT_ID="no-hasher"  # explicit, never a silent empty stamp
HOST="$(hostname -s 2>/dev/null || echo unknown)"

leg_names=() leg_status=() leg_detail=() leg_secs=()
FAILED_LEGS=()
record() {
    leg_names+=("$1"); leg_status+=("$2"); leg_detail+=("$3"); leg_secs+=("$4")
    [ "$2" = "FAIL" ] && FAILED_LEGS+=("$1")
}

cd "$WT" || die "cannot cd into worktree"

run_hygiene() {
    echo "== leg: hygiene"
    local t=$SECONDS log="$LOGDIR/hygiene.log"
    if git grep -nE '^(<{7} |>{7} )' -- . >"$log" 2>&1; then
        record hygiene FAIL "committed conflict markers — see hygiene.log" $((SECONDS - t))
    else
        record hygiene PASS "no conflict markers in the tracked tree" $((SECONDS - t))
    fi
}

run_fmt() {
    echo "== leg: fmt"
    local t=$SECONDS log="$LOGDIR/fmt.log"
    if cargo fmt --all -- --check >"$log" 2>&1; then
        record fmt PASS "cargo fmt --check" $((SECONDS - t))
    else
        record fmt FAIL "unformatted — see fmt.log" $((SECONDS - t))
    fi
}

run_clippy() {
    echo "== leg: clippy"
    local t=$SECONDS log="$LOGDIR/clippy.log"
    if cargo clippy --workspace --all-targets -- -D warnings >"$log" 2>&1; then
        record clippy PASS "clippy -D warnings" $((SECONDS - t))
    else
        record clippy FAIL "see clippy.log" $((SECONDS - t))
    fi
}

run_tests() {
    echo "== leg: tests"
    local t=$SECONDS log="$LOGDIR/tests.log"
    if cargo test --workspace >"$log" 2>&1; then
        record tests PASS "cargo test --workspace ($(grep -c 'test result: ok' "$log") suites ok)" $((SECONDS - t))
    else
        record tests FAIL "see tests.log" $((SECONDS - t))
    fi
}

STARTED=$SECONDS
want_leg hygiene && run_hygiene
want_leg fmt     && run_fmt
want_leg clippy  && run_clippy
want_leg tests   && run_tests
WALL=$((SECONDS - STARTED))

green=0; red=0
for st in "${leg_status[@]}"; do case "$st" in PASS) green=$((green+1)) ;; FAIL) red=$((red+1)) ;; esac; done
VERDICT="$green legs green"
[ "$red" = 0 ] || VERDICT="$red leg(s) RED — $green green"

echo
echo "=================================================="
printf '%-14s %-5s %-8s %s\n' "LEG" "RES" "SECS" "DETAIL"
printf '%-14s %-5s %-8s %s\n' "---" "---" "----" "------"
for i in "${!leg_names[@]}"; do
    printf '%-14s %-5s %-8s %s\n' "${leg_names[$i]}" "${leg_status[$i]}" "${leg_secs[$i]}" "${leg_detail[$i]}"
done
echo "=================================================="
echo "VERDICT: $VERDICT (${WALL}s)"

compose_comment() {
    local body="$LOGDIR/comment.md"
    {
        if [ -n "$PR_NUMBER" ]; then
            echo "### Local CI gate — PR #${PR_NUMBER}: ${PR_TITLE}"
            echo
        else
            echo "### Local CI gate — ref \`${HEAD_REF}\`"
            echo
        fi
        echo "Run of \`scripts/pr-verify.sh\` (fleet pattern: Amperstrand/.github \`workflow-templates/local-ci-gate.md\` — GitHub Actions is org-blocked, so this comment is the offline verification evidence, produced on the pinned head SHA)."
        echo
        echo "- **Head:** \`${HEAD_REF}\` @ \`${HEAD_OID:0:10}\`"
        [ -n "$PR_NUMBER" ] && echo "- **Base:** \`main\` @ \`${BASE_SHA:0:10}\`"
        echo "- **Gate:** pr-verify \`$SCRIPT_ID\` on \`${HOST}\`, started $(date -u '+%Y-%m-%d %H:%M UTC'), wall ${WALL}s"
        echo "- **Logs:** \`${LOGDIR}\` (on the verifying host)"
        echo
        echo "| Leg | Result | Detail |"
        echo "| --- | --- | --- |"
        for i in "${!leg_names[@]}"; do
            echo "| ${leg_names[$i]} | **${leg_status[$i]}** | ${leg_detail[$i]} (${leg_secs[$i]}s) |"
        done
        echo
        echo "**Verdict: ${VERDICT}.**"
        echo
        if [ "${#wanted[@]}" != "${#LEGS[@]}" ]; then
            echo "> Note: narrowed run (--only) — legs not listed here did not run."
            echo
        fi
        echo "<details><summary>What this gate does <b>not</b> run (no vacuous green)</summary>"
        echo
        echo "- the Next.js frontend build"
        echo "- the TXT-registration script (\`ci/register-txt.mjs\` — talks to live DNS)"
        echo "- deploy targets and anything relay/nameserver-visible"
        echo
        echo "</details>"
    } >"$body"
    echo "$body"
}

if [ -n "$PR_NUMBER" ] && [ "$NO_POST" = 0 ]; then
    BODY="$(compose_comment)"
    if URL="$(gh pr comment "$PR_NUMBER" -R "$SLUG" --body-file "$BODY")"; then
        echo "== posted: $URL"
    else
        echo "== FAILED to post comment (body kept at $BODY)" >&2
        red=$((red + 1))
    fi
elif [ -n "$PR_NUMBER" ]; then
    echo "== --no-post: comment body kept at $(compose_comment)"
fi

[ "$red" = 0 ] && exit 0 || exit 1
