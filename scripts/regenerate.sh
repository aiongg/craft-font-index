#!/usr/bin/env bash
# Regenerates google-fonts-index.json and its .gz copy from google/fonts at a commit, then runs
# the tests against that checkout. Run by hand; review `git diff --stat` and commit.
#
#   scripts/regenerate.sh [<google/fonts commit sha>]    (default: the tip of main)
#
# GOOGLE_FONTS_CHECKOUT sets where the sparse checkout lives (about 4 GB).
set -euo pipefail
cd "$(dirname "$0")/.."

checkout="${GOOGLE_FONTS_CHECKOUT:-${XDG_CACHE_HOME:-$HOME/.cache}/google-fonts-index/fonts}"
remote=https://github.com/google/fonts.git
commit="${1:-$(git ls-remote "$remote" refs/heads/main | cut -f1)}"
if [[ ! $commit =~ ^[0-9a-f]{40}$ ]]; then
  echo "regenerate: need a full 40-character commit sha, got '$commit'" >&2
  exit 2
fi

if [ ! -d "$checkout/.git" ]; then
  mkdir -p "$checkout"
  git -C "$checkout" init -q
  git -C "$checkout" remote add origin "$remote"
fi
git -C "$checkout" sparse-checkout set --no-cone '/ofl/' '/apache/' '/ufl/' '/lang/Lib/gflanguages/data/'
echo "fetching google/fonts@$commit into $checkout"
git -C "$checkout" fetch -q --depth 1 --filter=blob:none origin "$commit"
git -C "$checkout" checkout -q --detach FETCH_HEAD
# Drop the objects of earlier commits so the checkout doesn't grow with every run.
git -C "$checkout" reflog expire --expire=now --all
git -C "$checkout" gc -q --prune=now

cargo run --release -q -- --repo "$checkout" --commit "$commit" --out google-fonts-index.json
GOOGLE_FONTS_CHECKOUT="$checkout" cargo test --release -q -- --include-ignored
git diff --stat
