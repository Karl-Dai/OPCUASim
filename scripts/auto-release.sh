#!/usr/bin/env bash
# auto-release.sh — Detect unreleased commits on master and cut a release.
#
# Triggered daily by .github/workflows/auto-release.yml (Beijing 00:00).
# If HEAD has commits after the latest `v*` tag, this script:
#   1. decides the next version (minor if any `feat:`, else patch)
#   2. bumps version in all crates/*/Cargo.toml + Cargo.lock
#   3. appends a CHANGELOG.md section (bilingual headings, English items)
#   4. commits, creates an annotated tag, and pushes both
# Pushing the tag triggers the existing release.yml build/publish workflow.
#
# Usage:
#   scripts/auto-release.sh            # real run (pushes!)
#   scripts/auto-release.sh --dry-run  # print what WOULD happen, change nothing
#
# Notes:
#   * Must run on `master` with the full git history (fetch-depth: 0).
#   * Never force-pushes; aborts if the computed tag already exists.
#   * Only top-level `version` fields are touched — dependency versions are not.
#   * Push auth: uses the credential configured by actions/checkout
#     (GITHUB_TOKEN with contents: write, or a PAT in
#     secrets.AUTO_RELEASE_TOKEN when provided by the workflow).

set -euo pipefail

DRY_RUN=false
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=true

# Portable sed -i (BSD on macOS, GNU on Linux CI).
sedi() {
  if sed --version >/dev/null 2>&1; then sed -i "$@"; else sed -i '' "$@"; fi
}

# ---------------------------------------------------------------------------
# Guards
# ---------------------------------------------------------------------------
CURRENT_BRANCH="$(git branch --show-current)"
if [[ "$CURRENT_BRANCH" != "master" ]]; then
  echo "✋ auto-release only runs on master (current: $CURRENT_BRANCH); aborting." >&2
  exit 0
fi

LAST_TAG="$(git tag -l 'v*' --sort=-v:refname | head -1 || true)"
if [[ -z "$LAST_TAG" ]]; then
  echo "✋ no v* tags found; bootstrap the first release manually. aborting." >&2
  exit 0
fi

# ---------------------------------------------------------------------------
# Anything to release?
# ---------------------------------------------------------------------------
NEW_COUNT="$(git rev-list --no-merges --count "$LAST_TAG"..HEAD 2>/dev/null || true)"
if [[ "$NEW_COUNT" -eq 0 ]]; then
  echo "✅ nothing new since $LAST_TAG — no release needed."
  exit 0
fi

# ---------------------------------------------------------------------------
# Next version: minor on feat:, else patch
# ---------------------------------------------------------------------------
BASE="${LAST_TAG#v}"
MAJOR="${BASE%%.*}"; REST="${BASE#*.}"
MINOR="${REST%%.*}"; PATCH="${REST#*.}"
MINOR="${MINOR:-0}"; PATCH="${PATCH:-0}"

HAS_FEAT="$(git log --pretty=format:%s --no-merges "$LAST_TAG"..HEAD | grep -cE '^feat(\(|:|\b)' || true)"
if [[ "$HAS_FEAT" -gt 0 ]]; then
  NEW_MINOR=$((10#$MINOR + 1)); NEW_PATCH=0
else
  NEW_MINOR="$MINOR"; NEW_PATCH=$((10#$PATCH + 1))
fi
NEW_VERSION="$MAJOR.$NEW_MINOR.$NEW_PATCH"
NEW_TAG="v$NEW_VERSION"
DATE="$(date +%Y-%m-%d)"

if git rev-parse --verify "refs/tags/$NEW_TAG" >/dev/null 2>&1; then
  echo "✋ tag $NEW_TAG already exists; aborting (no force-push)." >&2
  exit 1
fi

echo "==> releasing $NEW_TAG from $LAST_TAG ($NEW_COUNT commits, feat=$HAS_FEAT)"

# ---------------------------------------------------------------------------
# Classify subjects
# ---------------------------------------------------------------------------
classify() { case "$1" in
  feat*) echo added;; fix*) echo fixed;; perf*|refactor*) echo refactored;;
  style*) echo changed;; docs*) echo docs;; test*) echo tests;;
  *) echo internal;; esac
}
clean_subject() {  # "feat(core): do stuff" → "Do stuff"
  local s="${1#*: }"; s="${s%%.*}"
  printf '%s' "$(tr '[:lower:]' '[:upper:]' <<<"${s:0:1}")${s:1}"
}

SUBJECTS="$(git log --pretty=format:%s --no-merges "$LAST_TAG"..HEAD | grep -v '^$')"

ADDED=(); CHANGED=(); FIXED=(); TESTS=(); INTERNAL=()
while IFS= read -r subj; do
  c="$(classify "$subj")"
  item="- $(clean_subject "$subj")."
  case "$c" in
    added) ADDED+=("$item");; fixed) FIXED+=("$item");;
    refactored|changed) CHANGED+=("$item");;
    tests) TESTS+=("$item");; *) INTERNAL+=("$item");;
  esac
done <<< "$SUBJECTS"

# ---------------------------------------------------------------------------
# Build CHANGELOG section (bilingual headings, English items)
# ---------------------------------------------------------------------------
SECTION="## [$NEW_VERSION] - $DATE

### Highlights / 亮点

- Automated release of $NEW_COUNT commit(s) since $LAST_TAG. / 自 $LAST_TAG 以来的 $NEW_COUNT 个提交自动发布.

"
top="$(git log --pretty=format:%s --no-merges "$LAST_TAG"..HEAD | grep -E '^(feat|fix)(\(|:|\b)' | head -3 || true)"
while IFS= read -r subj; do
  [[ -z "$subj" ]] && continue
  SECTION+="- $(clean_subject "$subj").
"
done <<< "$top"

emit_bucket() {  # heading, array name
  local heading="$1" arr="$2" line
  if (( $(eval "echo \${#${arr}[@]}") == 0 )); then return 0; fi
  eval 'set -- "${'"$arr"'[@]}"'
  SECTION+="
### $heading

"
  for line in "$@"; do SECTION+="$line
"; done
}
emit_bucket "Added 新增" ADDED
emit_bucket "Changed 改进" CHANGED
emit_bucket "Fixed 修复" FIXED
emit_bucket "Tests 测试" TESTS
emit_bucket "Internal 内部" INTERNAL

# ---------------------------------------------------------------------------
# Apply (real run only)
# ---------------------------------------------------------------------------
apply() {
  for f in crates/*/Cargo.toml; do
    sedi "s/^version = \"$BASE\"$/version = \"$NEW_VERSION\"/" "$f"
  done
  for pkg in opcuasim-core opcuamaster-egui opcuaserver-egui opcuaegui-shared; do
    sedi "/^name = \"$pkg\"$/{n;s/^version = \"$BASE\"$/version = \"$NEW_VERSION\"/;}" Cargo.lock
  done
  cargo check --workspace --quiet 2>/dev/null || true  # keep Cargo.lock consistent

  # Prepend the section right above the first "## [x.y.z]" heading.
  # (env var, not -v: awk rejects embedded newlines in -v strings)
  SECTION="$SECTION" awk '
    BEGIN { sec = ENVIRON["SECTION"] }
    /^## \[/ && !done { print sec; done=1 }
    { print }
    END { if (!done) print sec }
  ' CHANGELOG.md > CHANGELOG.md.tmp && mv CHANGELOG.md.tmp CHANGELOG.md
}

if $DRY_RUN; then
  echo ""
  echo "--- DRY RUN — would apply: ---"
  echo "  version bump: $BASE -> $NEW_VERSION (crates/*/Cargo.toml + Cargo.lock)"
  echo "  CHANGELOG section:"
  printf '%s\n' "$SECTION"
  echo "  commit: chore(release): $NEW_TAG — auto release ($NEW_COUNT commits since $LAST_TAG)"
  echo "  tag: $NEW_TAG (annotated) + push master"
  exit 0
fi

apply

git add -A
GIT_CMD=(git -c user.name="OPCUASim Release Bot" -c user.email="actions@users.noreply.github.com")
"${GIT_CMD[@]}" commit -m "chore(release): $NEW_TAG — auto release ($NEW_COUNT commits since $LAST_TAG)

Auto-generated by scripts/auto-release.sh on schedule (Beijing 00:00).
Full section in CHANGELOG.md." >/dev/null

"${GIT_CMD[@]}" tag -a "$NEW_TAG" -m "OPCUASim $NEW_TAG — auto release.

See CHANGELOG.md section [$NEW_VERSION] - $DATE for details."

# Push. Default auth comes from actions/checkout (GITHUB_TOKEN with
  # contents: write); optionally override with a PAT so tag pushes work
  # even where GITHUB_TOKEN is restricted.
  if [[ -n "${AUTO_RELEASE_TOKEN:-}" ]]; then
    git remote set-url origin "https://x-access-token:${AUTO_RELEASE_TOKEN}@github.com/${GITHUB_REPOSITORY:-OPCUASim/OPCUASim}.git"
  fi
  git push origin master
  git push origin "$NEW_TAG"

echo "✅ released $NEW_TAG (release.yml build/publish triggered by tag push)."