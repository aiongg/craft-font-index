//! The Noto index: the Noto families Google Fonts doesn't carry, or carries in fewer styles.
//!
//! Reads a mirror made by `scripts/fetch-noto.sh`:
//!
//! - `github.io/`: notofonts/notofonts.github.io at a commit. Its `state.json` lists every
//!   family of every script repository with its latest release, and its `fonts/` tree holds the
//!   released files.
//! - `noto-cjk-sans/`, `noto-cjk-serif/`: notofonts/noto-cjk at a Sans and a Serif release tag,
//!   for the per-region OTFs.
//! - `licenses/<repo>/<tag>/OFL.txt`: each script repository's licence at the release tag.
//! - `manifest.json`: the commit and tags.
//!
//! Only fonts whose name table names the SIL Open Font License are listed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};

use crate::font::{FontInfo, read_font};
use crate::lang::LangData;
use crate::licence::{copyright_header, reserved_names};
use crate::sample;
use crate::tags::{Curation, from_category};
use crate::{
    IndexHeader, MAX_FONT_BYTES, file_record, finish_index, is_commit, is_ref, license_record,
};

pub const SOURCE_ID: &str = "notofonts";
pub const REPO: &str = "notofonts/notofonts.github.io";
pub const CJK_REPO: &str = "notofonts/noto-cjk";

/// The builds taken from `fonts/<family>/`, best first. `full` adds Latin and other common
/// glyphs to the script, so it works without a fallback font.
const BUILDS: &[&str] = &[
    "full/variable-ttf",
    "full/ttf",
    "unhinted/variable-ttf",
    "unhinted/ttf",
];

/// noto-cjk directories per tag kind, and the script of each region suffix.
const CJK_DIRS: &[(&str, &[&str])] = &[
    (
        "sans",
        &[
            "Sans/OTF/Japanese",
            "Sans/OTF/Korean",
            "Sans/OTF/SimplifiedChinese",
            "Sans/OTF/TraditionalChinese",
            "Sans/OTF/TraditionalChineseHK",
            "Sans/Mono",
        ],
    ),
    (
        "serif",
        &[
            "Serif/OTF/Japanese",
            "Serif/OTF/Korean",
            "Serif/OTF/SimplifiedChinese",
            "Serif/OTF/TraditionalChinese",
            "Serif/OTF/TraditionalChineseHK",
        ],
    ),
];
const CJK_REGIONS: &[(&str, &str)] = &[
    ("JP", "Jpan"),
    ("KR", "Kore"),
    ("SC", "Hans"),
    ("TC", "Hant"),
    ("HK", "Hant"),
];

/// What `fetch-noto.sh` pinned.
pub struct Manifest {
    pub notofonts_commit: String,
    pub cjk_sans_tag: String,
    pub cjk_serif_tag: String,
}

impl Manifest {
    pub fn load(mirror: &Path) -> Result<Manifest, String> {
        let v = read_json(&mirror.join("manifest.json"))?;
        let s = |k: &str| {
            v[k].as_str()
                .filter(|s| is_ref(s))
                .map(str::to_string)
                .ok_or_else(|| format!("manifest.json: bad `{k}`"))
        };
        let m = Manifest {
            notofonts_commit: s("notofonts_commit")?,
            cjk_sans_tag: s("cjk_sans_tag")?,
            cjk_serif_tag: s("cjk_serif_tag")?,
        };
        if !is_commit(&m.notofonts_commit) {
            return Err("manifest.json: `notofonts_commit` is not a full sha".into());
        }
        Ok(m)
    }
}

/// How many families each step kept or dropped, for the generator's summary.
#[derive(Default, Debug)]
pub struct Counts {
    pub listed: usize,
    pub same_in_google_fonts: usize,
    pub not_ofl: usize,
    pub unusable: usize,
}

fn read_json(path: &Path) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_file() || m.len() > MAX_FONT_BYTES {
        return Err(format!(
            "{} is not a regular file of acceptable size",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A path from state.json: `fonts/<family dir>/<build>/<kind>/<file>`.
fn is_font_path(p: &str) -> bool {
    let parts: Vec<&str> = p.split('/').collect();
    parts.len() == 5
        && parts[0] == "fonts"
        && parts[1..4].iter().all(|s| is_ref(s))
        && !parts[4].is_empty()
        && parts[4] != ".."
        && parts[4]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-[],".contains(&b))
        && p.ends_with(".ttf")
}

/// A family's style coverage: for each file, whether it's italic and the weight and width
/// ranges it covers.
fn coverage(files: &[Value]) -> Vec<(bool, [f64; 4])> {
    files
        .iter()
        .map(|f| {
            let axis = |tag: &str, default: f64| {
                f["axes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| a["tag"] == tag)
                    .map_or((default, default), |a| {
                        (
                            a["min"].as_f64().unwrap_or(default),
                            a["max"].as_f64().unwrap_or(default),
                        )
                    })
            };
            let (wlo, whi) = axis("wght", f["weight"].as_f64().unwrap_or(400.0));
            let (dlo, dhi) = axis("wdth", f["width"].as_f64().unwrap_or(100.0));
            (f["style"] == "italic", [wlo, whi, dlo, dhi])
        })
        .collect()
}

/// Whether `ours` has a style, weight or width that no file of `theirs` covers.
pub fn adds_styles(ours: &[Value], theirs: &[Value]) -> bool {
    const EPS: f64 = 0.5;
    let theirs = coverage(theirs);
    coverage(ours).iter().any(|(italic, b)| {
        !theirs.iter().any(|(ti, t)| {
            ti == italic
                && t[0] - EPS <= b[0]
                && b[1] <= t[1] + EPS
                && t[2] - EPS <= b[2]
                && b[3] <= t[3] + EPS
        })
    })
}

/// Family names compare without case, spaces or punctuation: Google Fonts and Noto spell some
/// differently (`Noto Sans PhagsPa`, `Noto Sans Phags Pa`).
fn name_key(family: &str) -> String {
    family
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn is_ofl(info: &FontInfo) -> bool {
    let l = info.license.to_lowercase();
    l.contains("open font license") || l.contains("openfontlicense") || l.contains("/ofl")
}

fn category(family: &str) -> &'static str {
    let has = |w: &str| family.split(' ').any(|x| x == w);
    if has("Mono") {
        "MONOSPACE"
    } else if has("Serif") || has("Naskh") || has("Nastaliq") || has("Rashi") {
        "SERIF"
    } else if has("Sans") || has("Kufi") {
        "SANS_SERIF"
    } else {
        "DISPLAY"
    }
}

/// The copyright lines at the top of the licence file at `path`, if it has any.
fn licence_header(path: &Path) -> Option<String> {
    let m = std::fs::symlink_metadata(path).ok()?;
    if !m.is_file() || m.len() > 1024 * 1024 {
        return None;
    }
    copyright_header(&String::from_utf8_lossy(&std::fs::read(path).ok()?))
}

struct Candidate {
    family: String,
    files: Vec<Value>,
    cmap: BTreeSet<u32>,
    designer: Option<String>,
    scripts: BTreeSet<String>,
    sample_script: Option<String>,
    license_file: Value,
    /// The copyright notice at the top of the licence file.
    copyright: Option<String>,
    /// The family's own `repo` and `ref`, when they aren't the index's.
    repo_ref: Option<(String, String)>,
}

struct Parts {
    family: String,
    files: Vec<Value>,
    cmap: BTreeSet<u32>,
    designer: Option<String>,
    sample_script: Option<String>,
}

/// A family as read from its files.
struct Group {
    family: String,
    /// Each file's record and code points.
    files: Vec<(Value, BTreeSet<u32>)>,
    designer: Option<String>,
}

/// The build (`full/ttf`) of a state.json path.
fn build_of(path: &str) -> String {
    path.split('/')
        .skip(2)
        .take(2)
        .collect::<Vec<_>>()
        .join("/")
}

impl Group {
    /// Keeps only the files of the best build the family has.
    fn best_build(mut self) -> Group {
        if let Some(b) = BUILDS.iter().find(|b| {
            self.files
                .iter()
                .any(|(f, _)| f["path"].as_str().is_some_and(|p| build_of(p) == **b))
        }) {
            self.files
                .retain(|(f, _)| f["path"].as_str().is_some_and(|p| build_of(p) == *b));
        }
        self
    }

    /// The family name, its files with their samples ([`sample::pick`] for `primary`), the
    /// union of their code points, the designer and the family's `sample_script`.
    fn into_parts(self, lang: &LangData, primary: Option<&str>) -> Parts {
        let mut cmap = BTreeSet::new();
        let mut files = Vec::new();
        for (mut f, c) in self.files {
            sample::apply(&mut f, sample::pick(&lang.samples, primary, &c));
            cmap.extend(c);
            files.push(f);
        }
        files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let sample_script = sample::settle(&mut files, primary);
        Parts {
            family: self.family,
            files,
            cmap,
            designer: self.designer,
            sample_script,
        }
    }
}

/// Reads `paths` (relative to `root`, published at the same paths) and groups them by file
/// name stem (`NotoSansTamilUI` in `NotoSansTamilUI-CondensedBold.ttf`). One state.json family
/// can hold several (`Noto Sans Tamil` holds `Noto Sans Tamil UI`). A group takes the shortest
/// family name its fonts give, because static width and weight files often put those in the
/// family name (`Noto Sans Tamil UI Condensed`). Unusable files are warnings; a family with a
/// file that isn't under the OFL is left out.
fn read_files(
    root: &Path,
    paths: &[String],
    label: &str,
    counts: &mut Counts,
    warnings: &mut Vec<String>,
) -> Vec<Group> {
    let mut groups: BTreeMap<String, (Group, bool)> = BTreeMap::new();
    for p in paths {
        let local = root.join(p);
        let ok = std::fs::symlink_metadata(&local)
            .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= MAX_FONT_BYTES);
        if !ok {
            warnings.push(format!(
                "{label}: `{p}` is missing or not a regular file of acceptable size"
            ));
            continue;
        }
        let data = match std::fs::read(&local) {
            Ok(d) => d,
            Err(e) => {
                warnings.push(format!("{label}: read `{p}`: {e}"));
                continue;
            }
        };
        let info = read_font(&data);
        if info.axes.is_none() {
            warnings.push(format!("{label}: `{p}` does not parse as a font"));
            continue;
        }
        let file_name = p.rsplit('/').next().unwrap_or(p);
        let stem = file_name
            .split(['-', '[', '.'])
            .next()
            .unwrap_or(file_name)
            .to_string();
        let (g, not_ofl) = groups.entry(stem).or_insert_with(|| {
            let g = Group {
                family: String::new(),
                files: Vec::new(),
                designer: None,
            };
            (g, false)
        });
        if let Some(f) = &info.family
            && (g.family.is_empty() || (f.len(), f) < (g.family.len(), &g.family))
        {
            g.family = f.clone();
        }
        if !is_ofl(&info) {
            *not_ofl = true;
            continue;
        }
        let cmap = info.codepoints.clone();
        if g.designer.is_none() {
            g.designer = info.designer.clone();
        }
        let style = if info.italic { "italic" } else { "normal" };
        let weight = info.weight.round().clamp(1.0, 1000.0) as u32;
        g.files.push((
            file_record(p.clone(), &data, info, style, weight, None),
            cmap,
        ));
    }
    let mut out = Vec::new();
    for (_, (mut g, not_ofl)) in groups {
        if g.family.is_empty() {
            g.family = label.to_string();
        }
        if not_ofl {
            counts.not_ofl += 1;
            warnings.push(format!(
                "{}: left out, not every file is under the OFL",
                g.family
            ));
        } else if g.files.is_empty() {
            counts.unusable += 1;
        } else {
            out.push(g);
        }
    }
    out
}

fn wanted(only: Option<&[String]>, family: &str) -> bool {
    only.is_none_or(|o| o.iter().any(|x| name_key(x) == name_key(family)))
}

fn script_candidates(
    mirror: &Path,
    lang: &LangData,
    only: Option<&[String]>,
    counts: &mut Counts,
    warnings: &mut Vec<String>,
) -> Result<Vec<Candidate>, String> {
    let root = mirror.join("github.io");
    let state = read_json(&root.join("state.json"))?;
    let repos = state.as_object().ok_or("state.json is not an object")?;
    let mut out = Vec::new();
    let sorted: BTreeMap<&String, &Value> = repos.iter().collect();
    for (repo, entry) in sorted {
        if repo == "test" {
            continue;
        }
        let Some(fams) = entry["families"].as_object() else {
            continue;
        };
        if !is_ref(repo) {
            warnings.push(format!("state.json: unsafe repository name `{repo}`"));
            continue;
        }
        for (family, fam) in fams {
            // A cheap filter before reading: the families a state.json family holds start
            // with its name.
            if only.is_some_and(|o| !o.iter().any(|x| name_key(x).starts_with(&name_key(family)))) {
                continue;
            }
            let Some(tag) = fam["latest_release"]["url"]
                .as_str()
                .and_then(|u| u.rsplit_once("/releases/tag/"))
                .map(|(_, t)| t)
            else {
                warnings.push(format!("{family}: no release"));
                continue;
            };
            if !is_ref(tag) {
                warnings.push(format!("{family}: unsafe release tag `{tag}`"));
                continue;
            }
            let all: Vec<&str> = fam["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            // Every build is read, because a family's sub-families (`Noto Sans Tamil UI` in
            // `Noto Sans Tamil`) can lack the build the main family has.
            let paths: Vec<String> = all
                .iter()
                .filter(|p| is_font_path(p) && BUILDS.contains(&build_of(p).as_str()))
                .map(|p| p.to_string())
                .collect();
            if paths.is_empty() {
                warnings.push(format!("{family}: no TTF build"));
                continue;
            }
            let lic = mirror.join("licenses").join(repo).join(tag).join("OFL.txt");
            let groups = read_files(&root, &paths, family, counts, warnings);
            let groups: Vec<Group> = groups
                .into_iter()
                .filter(|g| wanted(only, &g.family))
                .collect();
            if groups.is_empty() {
                continue;
            }
            let license_file =
                match license_record(&lic, "OFL.txt", Some((&format!("notofonts/{repo}"), tag))) {
                    Ok(l) => l,
                    Err(e) => {
                        warnings.push(format!("{family}: {e}"));
                        counts.unusable += groups.len();
                        continue;
                    }
                };
            let copyright = licence_header(&lic);
            // The repository's script, for Noto what METADATA.pb's primary_script is for
            // Google Fonts.
            let primary = lang.subset_script(repo);
            for g in groups {
                let p = g.best_build().into_parts(lang, primary);
                out.push(Candidate {
                    family: p.family,
                    files: p.files,
                    cmap: p.cmap,
                    designer: p.designer,
                    scripts: primary.map(str::to_string).into_iter().collect(),
                    sample_script: p.sample_script,
                    license_file: license_file.clone(),
                    copyright: copyright.clone(),
                    repo_ref: None,
                });
            }
        }
    }
    Ok(out)
}

fn cjk_candidates(
    mirror: &Path,
    lang: &LangData,
    manifest: &Manifest,
    only: Option<&[String]>,
    counts: &mut Counts,
    warnings: &mut Vec<String>,
) -> Result<Vec<Candidate>, String> {
    let mut out = Vec::new();
    for (kind, dirs) in CJK_DIRS {
        let tag = if *kind == "sans" {
            &manifest.cjk_sans_tag
        } else {
            &manifest.cjk_serif_tag
        };
        let root = mirror.join(format!("noto-cjk-{kind}"));
        let mut paths = Vec::new();
        for d in *dirs {
            let rd = std::fs::read_dir(root.join(d)).map_err(|e| format!("noto-cjk {d}: {e}"))?;
            let mut names: Vec<String> = rd
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.ends_with(".otf") && is_ref(n))
                .collect();
            names.sort();
            paths.extend(names.into_iter().map(|n| format!("{d}/{n}")));
        }
        if let Some(o) = only
            && !o.iter().any(|x| name_key(x).contains("cjk"))
        {
            continue;
        }
        for g in read_files(&root, &paths, "noto-cjk", counts, warnings) {
            if !wanted(only, &g.family) {
                continue;
            }
            let region = CJK_REGIONS
                .iter()
                .find(|(r, _)| g.family.ends_with(&format!(" {r}")))
                .map(|(_, s)| *s);
            let p = g.into_parts(lang, region);
            let (family, files, cmap, designer) = (p.family, p.files, p.cmap, p.designer);
            // Serif releases keep the licence in `Serif/`, Sans releases at the top.
            let lic = [
                format!("{}/LICENSE", dirs[0].split('/').next().unwrap_or("")),
                "LICENSE".into(),
            ]
            .into_iter()
            .find(|p| root.join(p).is_file())
            .unwrap_or_else(|| "LICENSE".into());
            let license_file = match license_record(&root.join(&lic), &lic, None) {
                Ok(l) => l,
                Err(e) => {
                    warnings.push(format!("{family}: {e}"));
                    counts.unusable += 1;
                    continue;
                }
            };
            let scripts = CJK_REGIONS
                .iter()
                .filter(|(r, _)| family.ends_with(&format!(" {r}")))
                .map(|(_, s)| s.to_string())
                .collect();
            out.push(Candidate {
                family,
                files,
                cmap,
                designer,
                scripts,
                sample_script: p.sample_script,
                license_file,
                copyright: licence_header(&root.join(&lic)),
                repo_ref: Some((CJK_REPO.to_string(), tag.clone())),
            });
        }
    }
    Ok(out)
}

/// Builds the Noto index from `mirror`. `google` is the Google Fonts index, to leave out what
/// it already carries. With `only`, just the families named there.
pub fn build_index(
    mirror: &Path,
    lang: &LangData,
    google: &Value,
    only: Option<&[String]>,
    warnings: &mut Vec<String>,
) -> Result<(Value, Counts), String> {
    let manifest = Manifest::load(mirror)?;
    let curation = Curation::load_default()?;
    let mut counts = Counts::default();
    let gf: BTreeMap<String, &Value> = google["families"]
        .as_array()
        .ok_or("the Google Fonts index has no families")?
        .iter()
        .filter_map(|f| Some((name_key(f["family"].as_str()?), f)))
        .collect();
    let mut candidates = script_candidates(mirror, lang, only, &mut counts, warnings)?;
    candidates.extend(cjk_candidates(
        mirror,
        lang,
        &manifest,
        only,
        &mut counts,
        warnings,
    )?);

    let mut families = Vec::new();
    let mut seen = BTreeSet::new();
    for c in candidates {
        if !seen.insert(name_key(&c.family)) {
            warnings.push(format!(
                "{}: listed twice in state.json, kept the first",
                c.family
            ));
            continue;
        }
        let ours = gf.get(&name_key(&c.family));
        if let Some(g) = ours
            && !adds_styles(
                &c.files,
                g["files"].as_array().map_or(&[][..], Vec::as_slice),
            )
        {
            counts.same_in_google_fonts += 1;
            continue;
        }
        let languages: BTreeSet<String> = lang
            .covered(&c.cmap)
            .into_iter()
            .map(|l| l.id.clone())
            .collect();
        let mut scripts = c.scripts;
        scripts.extend(languages.iter().filter_map(|id| lang.script_of(id)));
        let (designer, categories) = match ours {
            Some(g) => (g["designer"].clone(), g["categories"].clone()),
            None => (
                json!(c.designer.unwrap_or_default()),
                json!([category(&c.family)]),
            ),
        };
        let tags: Vec<String> = match curation.get(&c.family) {
            Some(t) => t.to_vec(),
            None => match ours {
                Some(g) => g["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect(),
                None => from_category(category(&c.family))
                    .map(str::to_string)
                    .into_iter()
                    .collect(),
            },
        };
        let reserved = c
            .copyright
            .as_deref()
            .map(reserved_names)
            .unwrap_or_default();
        let mut fam = json!({
            "family": c.family,
            "source": SOURCE_ID,
            "designer": designer,
            "categories": categories,
            "subsets": [],
            "languages": languages,
            "scripts": scripts,
            "license": "OFL",
            "license_path": null,
            "license_file": c.license_file,
            "tags": tags,
            "reserved_names": reserved,
        });
        if let Some(cp) = &c.copyright {
            fam["copyright"] = json!(cp);
        }
        if let Some((repo, r)) = c.repo_ref {
            fam["repo"] = json!(repo);
            fam["ref"] = json!(r);
        }
        fam["sample_script"] = json!(c.sample_script);
        fam["files"] = json!(c.files);
        families.push(fam);
    }
    counts.listed = families.len();
    let header = IndexHeader {
        source: SOURCE_ID,
        name: "Noto",
        homepage: "https://notofonts.github.io",
        commit: &manifest.notofonts_commit,
        repo: REPO,
        base_urls: None,
    };
    Ok((finish_index(&header, families, lang), counts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(style: &str, weight: u32, axes: Value) -> Value {
        json!({"style": style, "weight": weight, "axes": axes})
    }

    #[test]
    fn adds_styles_compares_weight_width_and_italic() {
        let var = |lo: f64, hi: f64| {
            file(
                "normal",
                400,
                json!([{"tag": "wght", "min": lo, "max": hi}]),
            )
        };
        let gf = [var(400.0, 700.0)];
        assert!(!adds_styles(&[var(400.0, 700.0)], &gf));
        assert!(!adds_styles(&[file("normal", 700, json!([]))], &gf));
        assert!(adds_styles(&[var(100.0, 900.0)], &gf));
        assert!(adds_styles(&[file("italic", 400, json!([]))], &gf));
        let wide = file(
            "normal",
            400,
            json!([{"tag": "wght", "min": 400.0, "max": 700.0}, {"tag": "wdth", "min": 62.5, "max": 100.0}]),
        );
        assert!(adds_styles(std::slice::from_ref(&wide), &gf));
        assert!(!adds_styles(&gf, &[wide]));
    }

    #[test]
    fn state_paths_are_checked() {
        assert!(is_font_path(
            "fonts/NotoSansAdlam/full/variable-ttf/NotoSansAdlam[wght].ttf"
        ));
        assert!(!is_font_path("fonts/../full/ttf/x.ttf"));
        assert!(!is_font_path("fonts/NotoSansAdlam/full/ttf/../x.ttf"));
        assert!(!is_font_path("/etc/full/ttf/x/y.ttf"));
        assert!(!is_font_path("fonts/NotoSansAdlam/full/ttf/x.otf"));
    }
}
