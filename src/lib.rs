//! Generators for the Craft font indexes. The format is described in README.md.
//!
//! Each source has its own index: Google Fonts from a checkout of google/fonts ([`google`]), the
//! Noto releases from the notofonts repositories ([`noto`]). `sources.json` lists them
//! ([`sources`]). Every index is deterministic JSON. A malformed entry or a missing font file is
//! a warning, never an abort. Checkouts are untrusted input: symlinks are not followed and
//! nothing in them is executed.

pub mod font;
pub mod google;
pub mod lang;
pub mod noto;
pub mod pb;
pub mod sources;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::font::FontInfo;
use crate::lang::LangData;

pub use google::{LICENSE_DIRS, build_index};

/// Format version, `MAJOR.MINOR`. Readers reject an unknown major version; a minor version
/// only adds fields.
pub const VERSION: &str = "1.1";
/// The major version as a number, the field photocraft#2453's reader checks.
pub const SCHEMA: u32 = 1;

/// URL templates for files in a GitHub repository at a commit or tag: jsDelivr, then
/// raw.githubusercontent.com. jsDelivr refuses files over 20 MB, which the fallback serves.
pub const GITHUB_FILE_URLS: &[&str] = &[
    "https://cdn.jsdelivr.net/gh/{repo}@{ref}/{path}",
    "https://raw.githubusercontent.com/{repo}/{ref}/{path}",
];

pub(crate) const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LICENSE_BYTES: u64 = 1024 * 1024;

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn is_commit(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// What an index says about its source, besides its families.
pub(crate) struct IndexHeader<'a> {
    pub source: &'a str,
    pub name: &'a str,
    pub homepage: &'a str,
    /// The commit the index was generated from, and the default `{ref}`.
    pub commit: &'a str,
    /// The default `{repo}`.
    pub repo: &'a str,
    /// Format 1.0's `base_url` and `fallback_base_url`, for a source whose files all share them.
    pub base_urls: Option<(String, String)>,
}

/// One `files` entry. `extra_ps` is a PostScript name from outside the font (METADATA.pb).
pub(crate) fn file_record(
    path: String,
    data: &[u8],
    info: FontInfo,
    style: &str,
    weight: u32,
    extra_ps: Option<String>,
) -> Value {
    let mut ps: Vec<String> = Vec::new();
    ps.extend(extra_ps);
    ps.extend(info.postscript);
    ps.sort();
    ps.dedup();
    let axes: Vec<Value> = info
        .axes
        .unwrap_or_default()
        .into_iter()
        .map(|(t, lo, hi)| json!({"tag": t, "min": lo, "max": hi}))
        .collect();
    let mut rec = json!({
        "path": path,
        "size": data.len(),
        "sha256": sha256_hex(data),
        "style": style,
        "weight": weight,
        "axes": axes,
        "postscript": ps,
    });
    if (info.width - 100.0).abs() > 0.01 {
        rec["width"] = json!(info.width);
    }
    rec
}

/// A family's `license_file`: the file at `local`, published at `path` (in `repo_ref` when it
/// isn't the family's repository and ref).
pub(crate) fn license_record(
    local: &Path,
    path: &str,
    repo_ref: Option<(&str, &str)>,
) -> Result<Value, String> {
    let m = std::fs::symlink_metadata(local).map_err(|e| format!("licence `{path}`: {e}"))?;
    if !m.is_file() || m.len() == 0 || m.len() > MAX_LICENSE_BYTES {
        return Err(format!(
            "licence `{path}` is not a regular file of acceptable size"
        ));
    }
    let data = std::fs::read(local).map_err(|e| format!("licence `{path}`: {e}"))?;
    let mut rec = json!({"path": path, "size": data.len(), "sha256": sha256_hex(&data)});
    if let Some((repo, r)) = repo_ref {
        rec["repo"] = json!(repo);
        rec["ref"] = json!(r);
    }
    Ok(rec)
}

/// Sorts the families, builds the language and script tables and assembles the index.
pub(crate) fn finish_index(
    header: &IndexHeader,
    mut families: Vec<Value>,
    lang: &LangData,
) -> Value {
    families.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["family"].as_str().unwrap_or("").to_lowercase(),
                v["files"][0]["path"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    // Families refer to languages by position in the top-level table, which keeps the index
    // about half the size of one with language IDs repeated in every family.
    let used: BTreeSet<String> = families
        .iter()
        .flat_map(|f| f["languages"].as_array().into_iter().flatten())
        .filter_map(|l| l.as_str().map(str::to_string))
        .collect();
    let position: BTreeMap<&str, usize> = used
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for f in &mut families {
        let ids: Vec<usize> = f["languages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| position.get(l.as_str()?).copied())
            .collect();
        f["languages"] = json!(ids);
    }
    let languages: Vec<Value> = used
        .iter()
        .map(|id| match lang.languages.iter().find(|l| l.id == *id) {
            Some(l) => json!({
                "id": l.id,
                "name": l.name,
                "autonym": l.autonym,
                "script": l.script,
                "population": l.population,
                "historical": l.historical,
            }),
            None => {
                let script = lang.script_of(id).unwrap_or_default();
                json!({"id": id, "name": id, "autonym": null, "script": script, "population": 0, "historical": false})
            }
        })
        .collect();
    let used_scripts: BTreeSet<&str> = families
        .iter()
        .flat_map(|f| f["scripts"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let scripts: Vec<Value> = used_scripts
        .iter()
        .map(|code| json!({"id": code, "name": lang.scripts.get(*code).map_or(*code, String::as_str)}))
        .collect();
    let mut index = serde_json::Map::new();
    index.insert("schema".into(), json!(SCHEMA));
    index.insert("version".into(), json!(VERSION));
    index.insert("source".into(), json!(header.source));
    index.insert("commit".into(), json!(header.commit));
    if let Some((base, fallback)) = &header.base_urls {
        index.insert("base_url".into(), json!(base));
        index.insert("fallback_base_url".into(), json!(fallback));
    }
    index.insert(
        "source_info".into(),
        json!({
            "id": header.source,
            "name": header.name,
            "homepage": header.homepage,
            "curated": true,
            "file_urls": GITHUB_FILE_URLS,
        }),
    );
    index.insert("repo".into(), json!(header.repo));
    index.insert("ref".into(), json!(header.commit));
    index.insert("languages".into(), json!(languages));
    index.insert("scripts".into(), json!(scripts));
    index.insert("families".into(), json!(families));
    Value::Object(index)
}

/// The index as JSON with one table row or family per line, so a regeneration diffs per family.
pub fn to_json(index: &Value) -> String {
    let mut out = String::from("{");
    let Some(obj) = index.as_object() else {
        return index.to_string();
    };
    for (i, (k, v)) in obj.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&Value::String(k.clone()).to_string());
        out.push(':');
        match (k.as_str(), v.as_array()) {
            ("languages" | "scripts" | "families", Some(fams)) => {
                out.push_str("[\n");
                for (j, f) in fams.iter().enumerate() {
                    out.push_str(&f.to_string());
                    out.push_str(if j + 1 < fams.len() { ",\n" } else { "\n" });
                }
                out.push(']');
            }
            _ => out.push_str(&v.to_string()),
        }
    }
    out.push_str("}\n");
    out
}

/// Gzip with fixed settings and no timestamp, so the same JSON gives the same bytes.
pub fn gzip(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    enc.write_all(data).expect("writing to a Vec");
    enc.finish().expect("writing to a Vec")
}

/// The download URLs of `entry` (a file or a `license_file`) of `family`, best first: each
/// `source_info.file_urls` template with `{repo}` and `{ref}` taken from the entry, else the
/// family, else the index, and `{path}` percent-encoded per segment. An index without
/// `source_info` is format 1.0 and uses `base_url` and `fallback_base_url`.
pub fn file_urls(index: &Value, family: &Value, entry: &Value) -> Result<Vec<String>, String> {
    let path = entry["path"].as_str().ok_or("entry has no path")?;
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || path.contains('\\')
    {
        return Err(format!("unsafe path `{path}`"));
    }
    let encoded = path
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    let Some(templates) = index["source_info"]["file_urls"].as_array() else {
        return Ok([&index["base_url"], &index["fallback_base_url"]]
            .into_iter()
            .filter_map(Value::as_str)
            .map(|base| format!("{base}{encoded}"))
            .collect());
    };
    let field = |k: &str| {
        [entry, family, index]
            .into_iter()
            .find_map(|v| v[k].as_str())
            .map(str::to_string)
    };
    let mut urls = Vec::new();
    for t in templates {
        let t = t.as_str().ok_or("a file URL template is not a string")?;
        let mut url = t.replace("{path}", &encoded);
        for (key, check) in [("repo", is_repo as fn(&str) -> bool), ("ref", is_ref)] {
            let placeholder = format!("{{{key}}}");
            if url.contains(&placeholder) {
                let v = field(key).ok_or_else(|| format!("`{path}` has no `{key}`"))?;
                if !check(&v) {
                    return Err(format!("bad {key} `{v}`"));
                }
                url = url.replace(&placeholder, &v);
            }
        }
        if url.contains('{') || !url.starts_with("https://") {
            return Err(format!("template `{t}` does not resolve to an https URL"));
        }
        urls.push(url);
    }
    Ok(urls)
}

fn percent_encode(segment: &str) -> String {
    segment
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// A git ref (tag or commit) that needs no escaping in a URL.
pub fn is_ref(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// `owner/name` on GitHub.
pub fn is_repo(s: &str) -> bool {
    s.split_once('/')
        .is_some_and(|(o, n)| is_ref(o) && is_ref(n) && !n.contains('/'))
}
