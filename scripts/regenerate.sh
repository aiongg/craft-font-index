#!/usr/bin/env bash
# Regenerates both indexes and sources.json, then runs every test against the checkouts. Run by
# hand; review `git diff --stat` and commit.
#
#   scripts/regenerate.sh [<google/fonts commit> [<notofonts.github.io commit>]]
#
# Both default to the tip of main. GOOGLE_FONTS_CHECKOUT sets where the sparse google/fonts
# checkout lives (about 4 GB), NOTO_MIRROR where scripts/fetch-noto.sh puts the Noto files
# (about 4 GB). CJK_SANS_TAG and CJK_SERIF_TAG pin the noto-cjk releases.
set -euo pipefail
cd "$(dirname "$0")/.."
cache="${XDG_CACHE_HOME:-$HOME/.cache}/craft-font-index"
export GOOGLE_FONTS_CHECKOUT="${GOOGLE_FONTS_CHECKOUT:-$cache/google-fonts}"
export NOTO_MIRROR="${NOTO_MIRROR:-$cache/noto}"

checkout=$GOOGLE_FONTS_CHECKOUT
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
git -C "$checkout" sparse-checkout set --no-cone '/ofl/' '/apache/' '/ufl/' '/tags/' '/lang/Lib/gflanguages/data/'
echo "fetching google/fonts@$commit into $checkout"
git -C "$checkout" fetch -q --depth 1 --filter=blob:none origin "$commit"
git -C "$checkout" checkout -q --detach FETCH_HEAD
# Drop the objects of earlier commits so the checkout doesn't grow with every run.
git -C "$checkout" reflog expire --expire=now --all
git -C "$checkout" gc -q --prune=now

scripts/fetch-noto.sh ${2:+"$2"}

cargo run --release -q -- google --repo "$checkout" --commit "$commit" --out indexes/google-fonts.json
cargo run --release -q -- noto --mirror "$NOTO_MIRROR" --repo "$checkout" \
  --google-index indexes/google-fonts.json --out indexes/noto.json
cargo run --release -q -- sources --out sources.json indexes/google-fonts.json indexes/noto.json
cargo test --release -q -- --include-ignored
git diff --stat
