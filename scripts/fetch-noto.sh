#!/usr/bin/env bash
# Fetches what the Noto index is generated from into $NOTO_MIRROR:
#
#   github.io/        notofonts/notofonts.github.io at a commit: state.json and the full/ and
#                     unhinted/ TTF builds of every family
#   noto-cjk-sans/    notofonts/noto-cjk at a Sans release tag: the per-region OTFs and LICENSE
#   noto-cjk-serif/   the same at a Serif release tag
#   licenses/<repo>/<tag>/OFL.txt
#                     the licence of each script repository at its family's release tag
#   manifest.json     the commit and tags above
#
#   scripts/fetch-noto.sh [<notofonts.github.io commit sha>]   (default: the tip of main)
#
# CJK_SANS_TAG and CJK_SERIF_TAG pin the noto-cjk tags (default: the newest Sans<N> and
# Serif<N> tags). Run by hand; the checkouts take a few GB.
set -euo pipefail

mirror="${NOTO_MIRROR:-${XDG_CACHE_HOME:-$HOME/.cache}/craft-font-index/noto}"
io_remote=https://github.com/notofonts/notofonts.github.io.git
cjk_remote=https://github.com/notofonts/noto-cjk.git
commit="${1:-$(git ls-remote "$io_remote" refs/heads/main | cut -f1)}"
if [[ ! $commit =~ ^[0-9a-f]{40}$ ]]; then
  echo "fetch-noto: need a full 40-character commit sha, got '$commit'" >&2
  exit 2
fi
newest_tag() {
  git ls-remote --tags --refs "$cjk_remote" | sed 's|.*refs/tags/||' | grep -E "^$1[0-9]+\.[0-9]+$" | sort -V | tail -1
}
sans_tag="${CJK_SANS_TAG:-$(newest_tag Sans)}"
serif_tag="${CJK_SERIF_TAG:-$(newest_tag Serif)}"

# sparse <dir> <remote> <ref> <patterns...>: a shallow, blobless checkout of just <patterns>.
sparse() {
  local dir=$1 remote=$2 ref=$3
  shift 3
  if [ ! -d "$dir/.git" ]; then
    mkdir -p "$dir"
    git -C "$dir" init -q
    git -C "$dir" remote add origin "$remote"
  fi
  git -C "$dir" sparse-checkout set --no-cone "$@"
  echo "fetching $remote@$ref into $dir"
  git -C "$dir" fetch -q --depth 1 --filter=blob:none origin "$ref"
  git -C "$dir" checkout -q --detach FETCH_HEAD
  git -C "$dir" reflog expire --expire=now --all
  git -C "$dir" gc -q --prune=now
}

sparse "$mirror/github.io" "$io_remote" "$commit" \
  '/state.json' '/fonts/*/full/ttf/' '/fonts/*/full/variable-ttf/' \
  '/fonts/*/unhinted/ttf/' '/fonts/*/unhinted/variable-ttf/'
sparse "$mirror/noto-cjk-sans" "$cjk_remote" "refs/tags/$sans_tag" '/LICENSE' '/Sans/OTF/' '/Sans/Mono/'
sparse "$mirror/noto-cjk-serif" "$cjk_remote" "refs/tags/$serif_tag" '/LICENSE' '/Serif/LICENSE' '/Serif/OTF/'

# Each family's release tag names the script repository and the commit of its licence.
jq -r 'to_entries[] | .key as $repo | (.value.families // {}) | to_entries[]
       | .value.latest_release.url // empty | "\($repo) \(sub(".*/releases/tag/"; ""))"' \
  "$mirror/github.io/state.json" | sort -u |
  while read -r repo tag; do
    out="$mirror/licenses/$repo/$tag/OFL.txt"
    [ -s "$out" ] && continue
    mkdir -p "$(dirname "$out")"
    curl -sSfL --retry 3 -o "$out" "https://raw.githubusercontent.com/notofonts/$repo/$tag/OFL.txt" ||
      { rm -f "$out"; echo "fetch-noto: no OFL.txt in notofonts/$repo@$tag" >&2; }
  done

jq -n --arg c "$commit" --arg sans "$sans_tag" --arg serif "$serif_tag" \
  '{notofonts_commit: $c, cjk_sans_tag: $sans, cjk_serif_tag: $serif}' >"$mirror/manifest.json"
du -sh "$mirror"/*/.git "$mirror/licenses"
