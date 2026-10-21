# craft-font-index

Indexes of the font collections the Craft apps install from. The apps use them to find, download and verify fonts. Each index lists one source's families with their category, languages, scripts, licence and designer. For each font file it gives the location, size, sha256, style, weight, variation axes and PostScript names.

| Source | Index | What it lists |
|---|---|---|
| Google Fonts | `indexes/google-fonts.json` | Every family in [google/fonts](https://github.com/google/fonts) at a pinned commit |
| Noto | `indexes/noto.json` | The [Noto](https://notofonts.github.io) families Google Fonts doesn't carry, or carries in fewer styles: the full CJK fonts from [noto-cjk](https://github.com/notofonts/noto-cjk) releases, and script families, widths and UI variants missing from Google Fonts |

The repository holds only generated metadata and the generators, no font files. Clients download fonts from the source's repositories at the commit or tag the index records. Every file and licence in an index has a sha256. Only fonts under a licence that allows redistribution are listed: OFL, Apache 2.0 or the Ubuntu Font Licence for Google Fonts, OFL for Noto.

The design is @kntrttm's, from [storytold/photocraft#1135](https://github.com/storytold/photocraft/issues/1135) and [#2453](https://github.com/storytold/photocraft/pull/2453). The Google Fonts generator is based on that PR's `xtask fonts-index`, and the format extends its schema 1.

## Files

| File | Contents |
|---|---|
| `sources.json` | The built-in indexes, each with its size and sha256 |
| `indexes/<source>.json` | An index. Each language, script and family is on its own line, so a regeneration diffs per family. |
| `indexes/<source>.json.gz` | The same bytes, gzipped. Generated with mtime 0 and best compression, so the output is reproducible. |
| `src/` | The generators (see [Regenerating](#regenerating)) |
| `scripts/regenerate.sh` | Fetches both sources, regenerates every file and runs the tests |
| `scripts/fetch-noto.sh` | Fetches the Noto files the Noto index is generated from |

A client compiles in the sha256 of one `sources.json` and fetches it from a commit of this repository:

- `https://cdn.jsdelivr.net/gh/aiongg/craft-font-index@<commit>/sources.json`
- fallback: `https://raw.githubusercontent.com/aiongg/craft-font-index/<commit>/sources.json`

It then fetches each index from the same commit, at the `path` (or `gz_path`) `sources.json` gives, and checks it against the sha256 there.

## Format

The indexes and `sources.json` are JSON objects. All lists are sorted, and the same input always gives the same bytes.

### Versioning

`version` is `"MAJOR.MINOR"`. A client:

- rejects an index whose major version it doesn't know;
- ignores fields it doesn't know.

A minor version only adds fields. Any other change (removing or renaming a field, or changing a field's meaning or type) is a new major version. `schema` holds the major version as a number. photocraft#2453's reader checks that field.

The current version is `1.1`. Version 1.0 had no `source_info`, `repo`, `ref`, `license_file` or `width`, and only Google Fonts indexes. A 1.1 reader treats a 1.0 index as the Google Fonts source and builds file URLs from `base_url` and `fallback_base_url`. The Google Fonts index keeps every 1.0 field, so 1.0 readers still read it.

### sources.json

| Field | Type | Meaning |
|---|---|---|
| `schema`, `version` | | As in an index |
| `indexes` | array | The built-in indexes, in the order a client lists them |

Each entry of `indexes`:

| Field | Type | Meaning |
|---|---|---|
| `source` | string | The index's `source` |
| `name` | string | The index's `source_info.name` |
| `curated` | bool | The index's `source_info.curated` |
| `path` | string | The index, relative to `sources.json` |
| `size`, `sha256` | number, string | Of the index at `path` |
| `gz_path` | string | The gzipped index, relative to `sources.json` |
| `gz_size`, `gz_sha256` | number, string | Of the gzipped index |
| `commit` | string | The index's `commit` |
| `families`, `files` | number | How many families and font files the index lists |

### Top level

| Field | Type | Meaning |
|---|---|---|
| `schema` | number | The major version (`1`) |
| `version` | string | `"MAJOR.MINOR"`, see above |
| `source` | string | The source's ID: `google/fonts` or `notofonts`. Families from different indexes are told apart by it. |
| `commit` | string | The commit the index was generated from (40 hex characters): of google/fonts, or of notofonts/notofonts.github.io |
| `base_url` | string | Google Fonts only (format 1.0): `https://cdn.jsdelivr.net/gh/google/fonts@<commit>/`. A file's URL is this plus its `path`. |
| `fallback_base_url` | string | Google Fonts only (format 1.0): `https://raw.githubusercontent.com/google/fonts/<commit>/`, for when jsDelivr fails |
| `source_info` | object | 1.1. The source, see below |
| `repo` | string | 1.1. The default `{repo}` for file URLs: `owner/name` on GitHub |
| `ref` | string | 1.1. The default `{ref}` for file URLs: a commit or tag |
| `languages` | array | Every language any family supports, sorted by `id` (see below) |
| `scripts` | array | Every script any family uses, sorted by `id` (see below) |
| `families` | array | The families, sorted by name (case-insensitive) |

### Source info

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Same as the top-level `source` |
| `name` | string | Display name (`Google Fonts`, `Noto`) |
| `homepage` | string | https URL |
| `curated` | bool | `true` in the indexes this repository publishes. Clients set it to `false` for an index a user adds, whatever the file says. |
| `file_urls` | string array | URL templates for files, best first. A client tries them in order. |

A template contains `{path}` and may contain `{repo}` and `{ref}`. Both built-in indexes use:

```
https://cdn.jsdelivr.net/gh/{repo}@{ref}/{path}
https://raw.githubusercontent.com/{repo}/{ref}/{path}
```

To get a file's URLs, a client fills each template:

- `{path}`: the entry's `path`, each `/`-separated segment percent-encoded (everything except `A-Z a-z 0-9 - . _ ~`);
- `{repo}` and `{ref}`: from the entry (a file or a `license_file`) if it has them, else from its family, else from the top level.

A `repo` is `owner/name` and a `ref` is made of `A-Z a-z 0-9 . _ -`, so neither needs encoding. A client rejects a path with an empty, `.` or `..` segment, and a URL that isn't https after filling. jsDelivr refuses files over 20 MB (the Noto Serif CJK fonts), so the fallback serves those.

`craft_font_index::file_urls` implements these rules.

### Language

| Field | Type | Meaning |
|---|---|---|
| `id` | string | gflanguages ID: `<ISO 639 language>_<ISO 15924 script>`, sometimes with `_<region>` (`vi_Latn`, `ja_Jpan`, `bsc_Latn_GN`) |
| `name` | string | English name |
| `autonym` | string or null | The name in the language itself |
| `script` | string | ISO 15924 code |
| `population` | number | Speakers, from the lang data; `0` when unknown |
| `historical` | bool | A historical language |

### Script

| Field | Type | Meaning |
|---|---|---|
| `id` | string | ISO 15924 code. `Zmth` (maths), `Zsym` (symbols) and `Zsye` (emoji) cover google/fonts' `math`, `symbols` and `emoji` subsets. |
| `name` | string | English name |

### Family

| Field | Type | Meaning |
|---|---|---|
| `family` | string | Family name, unique in the index (case-insensitive). Indexes can share a name: Google Fonts' `Noto Sans JP` is a subset of Noto's `Noto Sans CJK JP`, but a family named the same in two indexes is the same design. |
| `source` | string | 1.1. The index's `source` |
| `designer` | string | Empty when not known |
| `categories` | string array | `SANS_SERIF`, `SERIF`, `DISPLAY`, `HANDWRITING` or `MONOSPACE` |
| `subsets` | string array | google/fonts subsets (`latin`, `latin-ext`, `japanese`, `menu`, ...); empty for other sources |
| `languages` | number array | Positions in the top-level `languages` array |
| `scripts` | string array | ISO 15924 codes, each in the top-level `scripts` array |
| `license` | string | `OFL`, `APACHE2` or `UFL` |
| `license_path` | string or null | Format 1.0: the licence file in google/fonts (`ofl/lato/OFL.txt`), relative to `base_url`. Null in other sources. |
| `license_file` | object or null | 1.1. The licence file: `path`, `size`, `sha256`, and `repo` and `ref` when it isn't in the family's repository at the family's ref. Located like a font file. Null only when google/fonts has no licence file for the family. |
| `repo`, `ref` | string | 1.1, optional. Where the family's files are, when that isn't the top-level `repo` and `ref` (the Noto CJK families are in notofonts/noto-cjk at a release tag). |
| `files` | array | The font files, sorted by path |

`languages` holds the languages the source declares (google/fonts METADATA.pb), plus every living language whose base exemplar characters are all in the family's character map. A living language is one with a known speaker population that is not marked historical. Families refer to languages by position because that halves the index size. `scripts` holds the scripts of those languages, plus the family's own script: METADATA.pb's `primary_script` and the scripts of its subsets for Google Fonts, the script of the notofonts repository or CJK region for Noto.

### File

| Field | Type | Meaning |
|---|---|---|
| `path` | string | Path in the family's repository, the `{path}` of the URL templates. In google/fonts it is `<ofl\|apache\|ufl>/<family dir>/<file>`, and all of a family's files are in one directory. It can contain `[`, `]` and `,` (`Roboto[wdth,wght].ttf`). |
| `size` | number | Bytes |
| `sha256` | string | 64 lowercase hex characters |
| `style` | string | `normal` or `italic` |
| `weight` | number | CSS weight (1 to 1000), from METADATA.pb for Google Fonts and from the font for Noto. For a variable font, this is the weight of the default instance. |
| `width` | number | 1.1, optional. CSS font-stretch percentage of the default instance, from the `wdth` axis or OS/2 `usWidthClass`. Present only when it isn't 100, so a family can list static files that differ only in width. |
| `axes` | array | Variation axes, `{"tag": "wght", "min": 100.0, "max": 900.0}`, read from the font's `fvar` table. Empty for a static font. |
| `postscript` | string array | PostScript names: METADATA.pb's `post_script_name`, the font's name ID 6 and, for a variable font, each named instance's PostScript name. When an instance has no name of its own, it is the name ID 25 prefix, `-` and the subfamily name without spaces. Missing-font lookup matches on these. |

A client verifies a downloaded file's size, then its sha256, then that it parses as a font, before writing it. It verifies a licence's size and sha256.

### Example

```json
{"schema":1,"version":"1.1","source":"notofonts","commit":"93fac8a...",
 "source_info":{"id":"notofonts","name":"Noto","homepage":"https://notofonts.github.io","curated":true,
   "file_urls":["https://cdn.jsdelivr.net/gh/{repo}@{ref}/{path}","https://raw.githubusercontent.com/{repo}/{ref}/{path}"]},
 "repo":"notofonts/notofonts.github.io","ref":"93fac8a...",
 "languages":[
{"id":"aa_Latn","name":"Afar","autonym":"Qafar","script":"Latn","population":2119662,"historical":false},
...],
 "scripts":[
{"id":"Cyrl","name":"Cyrillic"},
...],
 "families":[
{"family":"Noto Sans CJK JP","source":"notofonts","designer":"Ryoko NISHIZUKA ...","categories":["SANS_SERIF"],
 "subsets":[],"languages":[0,4,7,18],"scripts":["Cyrl","Hans","Hant","Jpan","Kore","Latn"],
 "license":"OFL","license_path":null,
 "license_file":{"path":"LICENSE","size":4301,"sha256":"6a73f954..."},
 "repo":"notofonts/noto-cjk","ref":"Sans2.004",
 "files":[{"path":"Sans/OTF/Japanese/NotoSansCJKjp-Black.otf","size":17828280,"sha256":"...",
   "style":"normal","weight":900,"axes":[],"postscript":["NotoSansCJKjp-Black"]},
   ...]},
{"family":"Noto Serif Myanmar","source":"notofonts","designer":"Google","categories":["SERIF"],
 "subsets":[],"languages":[0,2,3],"scripts":["Latn","Mymr"],
 "license":"OFL","license_path":null,
 "license_file":{"path":"OFL.txt","size":4383,"sha256":"246a7585...","repo":"notofonts/myanmar","ref":"NotoSerifMyanmar-v2.106"},
 "files":[{"path":"fonts/NotoSerifMyanmar/full/variable-ttf/NotoSerifMyanmar[wdth,wght].ttf","size":...,
   "sha256":"...","style":"normal","weight":400,
   "axes":[{"tag":"wght","min":100.0,"max":900.0},{"tag":"wdth","min":62.5,"max":100.0}],
   "postscript":["NotoSerifMyanmar-Black","NotoSerifMyanmar-Bold","..."]}]},
...]}
```

The second file's URLs are `https://cdn.jsdelivr.net/gh/notofonts/notofonts.github.io@93fac8a.../fonts/NotoSerifMyanmar/full/variable-ttf/NotoSerifMyanmar%5Bwdth%2Cwght%5D.ttf` and the raw.githubusercontent.com one.

## What the generators read

### Google Fonts

From a checkout of google/fonts (sparse: `ofl/`, `apache/`, `ufl/` and `lang/Lib/gflanguages/data/`):

- every `{ofl,apache,ufl}/*/METADATA.pb`, parsed as protobuf text format;
- every font file a METADATA.pb lists, which it hashes and reads with [skrifa](https://crates.io/crates/skrifa) for axes, PostScript names and the character map;
- each family's licence file, which it hashes;
- the gflanguages data, for language names, scripts and exemplar characters. The Noto generator uses it too.

### Noto

From the mirror `scripts/fetch-noto.sh` makes:

- `state.json` of [notofonts/notofonts.github.io](https://github.com/notofonts/notofonts.github.io) at a commit. It lists every family of every notofonts script repository, its latest release tag and its released files, which that repository's `fonts/` tree holds. The index takes the files from there, at that commit.
- [notofonts/noto-cjk](https://github.com/notofonts/noto-cjk) at the newest `Sans<version>` and `Serif<version>` release tags: the per-region OTFs (`Sans/OTF/<region>/`, `Sans/Mono/`, `Serif/OTF/<region>/`) in every weight, and the licence.
- Each script repository's `OFL.txt` at its family's release tag.

For each family it reads the `full` and `unhinted` TTF builds and groups the files by file name stem, because one state.json family can hold several (`Noto Sans Tamil` holds `Noto Sans Tamil UI`). Each group takes the first build it has of `full/variable-ttf`, `full/ttf`, `unhinted/variable-ttf`, `unhinted/ttf`; `full` adds Latin and common glyphs to the script, so it works without a fallback font. A group is named by the shortest family name its fonts give.

A family is left out when Google Fonts has a family of the same name (ignoring case, spaces and punctuation) whose files cover every style, weight range and width range of the Noto files. Fonts whose name table doesn't name the SIL Open Font License are left out.

Both generators treat a malformed entry, a missing or unparseable font file or an unsafe path as a warning and leave that entry out. They don't follow symlinks or run anything from the checkouts.

## Regenerating

The indexes are regenerated by hand. Nothing runs on a schedule.

```sh
scripts/regenerate.sh                          # the tips of google/fonts and notofonts.github.io
scripts/regenerate.sh <google/fonts commit> <notofonts.github.io commit>
```

The script keeps a sparse, shallow google/fonts checkout (about 4 GB) in `$GOOGLE_FONTS_CHECKOUT` and the Noto mirror (about 4 GB, 1.2 GB downloaded) in `$NOTO_MIRROR`, both by default under `~/.cache/craft-font-index/`. `CJK_SANS_TAG` and `CJK_SERIF_TAG` pin the noto-cjk releases. It writes both indexes and `sources.json`, runs all tests and prints `git diff --stat`. Review the diff, then commit with the commits and tags in the message.

The generator on its own:

```sh
cargo run --release -- google --repo <google/fonts checkout> --out indexes/google-fonts.json
cargo run --release -- noto --mirror <mirror> --repo <google/fonts checkout> \
  --google-index indexes/google-fonts.json --out indexes/noto.json
cargo run --release -- sources --out sources.json indexes/google-fonts.json indexes/noto.json
```

## Tests

`cargo test` checks the committed indexes against the format rules above, the Google Fonts index also against format 1.0's, and `sources.json` against the indexes. It checks that the gzipped copies match and that the generators turn broken and hostile checkout entries into warnings.

More tests need the checkouts at the indexes' commits, or the network:

```sh
GOOGLE_FONTS_CHECKOUT=<dir> NOTO_MIRROR=<dir> cargo test --release -- --ignored
```

- `regenerating_google_sample_matches` and `regenerating_noto_sample_matches` regenerate a few families (static and variable, each licence directory, CJK, static widths, sub-families) and compare them with the committed entries.
- `google_sample_files_match_sha256_and_size` checks every 20th Google Fonts file; `noto_sample_files_match_sha256_and_size` checks every 5th Noto file and every Noto licence.
- `downloads_match_index` downloads the three smallest files and a licence of each index from every URL template and checks their sha256.

## Licence

The generators are Apache-2.0 (see `LICENSE`). The Google Fonts index is derived from google/fonts METADATA.pb files and the gflanguages data, both Apache-2.0; the Noto index from notofonts' state.json and the fonts' name tables. Each font is under the licence its family's `license` and `license_file` name.
