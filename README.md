# google-fonts-index

An index of the [Google Fonts](https://github.com/google/fonts) collection at a pinned commit of `google/fonts`. The Craft apps use it to find, download and verify fonts. It lists each family's category, languages, scripts, licence and designer. For each font file it gives the path, size, sha256, style, weight, variation axes and PostScript names.

The repository holds only generated metadata and the generator, no font files. Clients download fonts from `google/fonts` at the commit the index records.

The design is @kntrttm's, from [storytold/photocraft#1135](https://github.com/storytold/photocraft/issues/1135) and [#2453](https://github.com/storytold/photocraft/pull/2453). The generator is based on that PR's `xtask fonts-index`, and the format extends its schema 1.

## Files

| File | Contents |
|---|---|
| `google-fonts-index.json` | The index. Each language, script and family is on its own line, so a regeneration diffs per family. |
| `google-fonts-index.json.gz` | The same bytes, gzipped. Generated with mtime 0 and best compression, so the output is reproducible. |
| `src/` | The generator: `cargo run --release -- --repo <google/fonts checkout> --out google-fonts-index.json` |
| `scripts/regenerate.sh` | Fetches google/fonts at a commit, regenerates the index and runs the tests |

Clients fetch the index from a commit of this repository and check it against a sha256 compiled into the client:

- `https://cdn.jsdelivr.net/gh/aiongg/google-fonts-index@<commit>/google-fonts-index.json`
- fallback: `https://raw.githubusercontent.com/aiongg/google-fonts-index/<commit>/google-fonts-index.json`

## Format

The index is one JSON object. All lists are sorted, and the same checkout always gives the same bytes.

### Versioning

`version` is `"MAJOR.MINOR"`. A client:

- rejects an index whose major version it doesn't know;
- ignores fields it doesn't know.

A minor version only adds fields. Any other change (removing or renaming a field, or changing a field's meaning or type) is a new major version. `schema` holds the major version as a number. photocraft#2453's reader checks that field.

The current version is `1.0`.

### Top level

| Field | Type | Meaning |
|---|---|---|
| `schema` | number | The major version (`1`) |
| `version` | string | `"MAJOR.MINOR"`, see above |
| `source` | string | `"google/fonts"` |
| `commit` | string | The google/fonts commit the index was generated from (40 hex characters) |
| `base_url` | string | `https://cdn.jsdelivr.net/gh/google/fonts@<commit>/`. A file's URL is this plus its `path`. |
| `fallback_base_url` | string | `https://raw.githubusercontent.com/google/fonts/<commit>/`, for when jsDelivr fails |
| `languages` | array | Every language any family supports, sorted by `id` (see below) |
| `scripts` | array | Every script any family uses, sorted by `id` (see below) |
| `families` | array | The families, sorted by name (case-insensitive) |

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
| `family` | string | Family name, unique in the index (case-insensitive) |
| `designer` | string | From METADATA.pb; empty when not given |
| `categories` | string array | `SANS_SERIF`, `SERIF`, `DISPLAY`, `HANDWRITING` or `MONOSPACE`, in METADATA.pb's order |
| `subsets` | string array | google/fonts subsets (`latin`, `latin-ext`, `japanese`, `menu`, ...) |
| `languages` | number array | Positions in the top-level `languages` array |
| `scripts` | string array | ISO 15924 codes, each in the top-level `scripts` array |
| `license` | string | `OFL`, `APACHE2` or `UFL` |
| `license_path` | string or null | The licence file in google/fonts (`ofl/lato/OFL.txt`), relative to `base_url` |
| `files` | array | The font files, sorted by path. All are in the same `<licence dir>/<family dir>/` directory. |

`languages` holds the languages METADATA.pb declares, plus every living language whose base exemplar characters are all in the family's character map. A living language is one with a known speaker population that is not marked historical. Families refer to languages by position because that halves the index size. `scripts` holds the scripts of those languages, the `primary_script` from METADATA.pb and the scripts of the family's subsets.

### File

| Field | Type | Meaning |
|---|---|---|
| `path` | string | Path in google/fonts, relative to `base_url`: `<ofl\|apache\|ufl>/<family dir>/<file>`. It can contain `[`, `]` and `,` (`Roboto[wdth,wght].ttf`), so percent-encode it for a URL. |
| `size` | number | Bytes |
| `sha256` | string | 64 lowercase hex characters |
| `style` | string | `normal` or `italic` |
| `weight` | number | CSS weight (1 to 1000) from METADATA.pb. For a variable font, this is the weight of the default instance. |
| `axes` | array | Variation axes, `{"tag": "wght", "min": 100.0, "max": 900.0}`, read from the font's `fvar` table. Empty for a static font. |
| `postscript` | string array | PostScript names: METADATA.pb's `post_script_name`, the font's name ID 6 and, for a variable font, each named instance's PostScript name. When an instance has no name of its own, it is the name ID 25 prefix, `-` and the subfamily name without spaces. Missing-font lookup matches on these. |

A client verifies a downloaded file's size, then its sha256, then that it parses as a font, before writing it.

### Example

```json
{"schema":1,"version":"1.0","source":"google/fonts","commit":"45e3d52...",
 "base_url":"https://cdn.jsdelivr.net/gh/google/fonts@45e3d52.../",
 "fallback_base_url":"https://raw.githubusercontent.com/google/fonts/45e3d52.../",
 "languages":[
{"id":"aa_Latn","name":"Afar","autonym":"Qafar","script":"Latn","population":2119662,"historical":false},
...],
 "scripts":[
{"id":"Adlm","name":"Adlam"},
...],
 "families":[
{"family":"Noto Sans JP","designer":"Google","categories":["SANS_SERIF"],
 "subsets":["cyrillic","japanese","latin","latin-ext","menu","vietnamese"],
 "languages":[0,4,7,18],"scripts":["Cyrl","Hira","Jpan","Kana","Latn"],
 "license":"OFL","license_path":"ofl/notosansjp/OFL.txt",
 "files":[{"path":"ofl/notosansjp/NotoSansJP[wght].ttf","size":9589900,
   "sha256":"c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f",
   "style":"normal","weight":400,"axes":[{"tag":"wght","min":100.0,"max":900.0}],
   "postscript":["NotoSansJP-Black","NotoSansJP-Bold","NotoSansJP-ExtraBold","NotoSansJP-ExtraLight",
     "NotoSansJP-Light","NotoSansJP-Medium","NotoSansJP-Regular","NotoSansJP-SemiBold","NotoSansJP-Thin"]}]},
...]}
```

## What the generator reads

From a checkout of google/fonts (sparse: `ofl/`, `apache/`, `ufl/` and `lang/Lib/gflanguages/data/`):

- every `{ofl,apache,ufl}/*/METADATA.pb`, parsed as protobuf text format;
- every font file a METADATA.pb lists, which it hashes and reads with [skrifa](https://crates.io/crates/skrifa) for axes, PostScript names and the character map;
- the gflanguages data, for language names, scripts and exemplar characters.

A malformed METADATA.pb, a missing or unparseable font file, or an unsafe filename is a warning, and the generator leaves that entry out. It doesn't follow symlinks or run anything from the checkout.

## Regenerating

The index is regenerated by hand. Nothing runs on a schedule.

```sh
scripts/regenerate.sh               # the tip of google/fonts main
scripts/regenerate.sh <commit sha>  # a given commit
```

The script keeps a sparse, shallow checkout (about 4 GB) in `$GOOGLE_FONTS_CHECKOUT`, by default `~/.cache/google-fonts-index/fonts`. It writes both index files, runs all tests and prints `git diff --stat`. Review the diff, then commit with the google/fonts commit in the message.

## Tests

`cargo test` checks the committed index against the format rules above, and checks that the gzipped copy matches. It also checks that the generator turns broken and hostile checkout entries into warnings.

Two more tests need a checkout at the index's commit:

```sh
GOOGLE_FONTS_CHECKOUT=<dir> cargo test --release -- --ignored
```

- `regenerating_sample_families_matches` regenerates six families (static and variable, each licence directory) and compares them with the committed entries.
- `sample_files_match_sha256_and_size` checks size and sha256 for every 20th file in the index.

## Licence

The generator is Apache-2.0 (see `LICENSE`). The index is derived from google/fonts METADATA.pb files and the gflanguages data, both Apache-2.0. Each font is under the licence its family's `license` and `license_path` name.
