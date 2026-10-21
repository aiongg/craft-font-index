//! Builds the Google Fonts index from a checkout of google/fonts. The format is described in
//! README.md.
//!
//! Walks `{ofl,apache,ufl}/*/METADATA.pb`, hashes every listed font file, reads axes, PostScript
//! names and code points from the font itself, and writes deterministic JSON. A malformed
//! METADATA.pb or a missing font file is a warning, never an abort. The checkout is untrusted
//! input: symlinks are not followed and nothing in it is executed. Based on @kntrttm's
//! `xtask fonts-index` (photocraft#2453).

pub mod font;
pub mod lang;
pub mod pb;

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::lang::LangData;
use crate::pb::{Val, parse_pb};

/// Format version, `MAJOR.MINOR`. Readers reject an unknown major version; a minor version
/// only adds fields.
pub const VERSION: &str = "1.0";
/// The major version as a number, the field photocraft#2453's reader checks.
pub const SCHEMA: u32 = 1;

pub const LICENSE_DIRS: &[&str] = &["ofl", "apache", "ufl"];
const LICENSE_FILES: &[&str] = &["OFL.txt", "LICENSE.txt", "UFL.txt"];
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds the index for the checkout at `repo`. With `only`, just the family directories named
/// there (`ofl/lato`, or `lato` in any licence directory). Per-family problems go to `warnings`.
pub fn build_index(
    repo: &Path,
    commit: &str,
    only: Option<&[String]>,
    warnings: &mut Vec<String>,
) -> Result<Value, String> {
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!("commit `{commit}` is not a full lowercase hex sha"));
    }
    let lang = LangData::load(repo, warnings)?;
    let mut families: Vec<Value> = Vec::new();
    for d in LICENSE_DIRS {
        let Ok(rd) = std::fs::read_dir(repo.join(d)) else {
            continue;
        };
        let mut dirs: Vec<_> = rd.filter_map(Result::ok).collect();
        dirs.sort_by_key(|e| e.file_name());
        for e in dirs {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(only) = only
                && !only
                    .iter()
                    .any(|o| *o == name || *o == format!("{d}/{name}"))
            {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            match family_entry(repo, d, &name, &lang, warnings) {
                Ok(Some(f)) => families.push(f),
                Ok(None) => {}
                Err(w) => warnings.push(format!("{d}/{name}: {w}")),
            }
        }
    }
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
    let position: std::collections::BTreeMap<&str, usize> = used
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
    Ok(json!({
        "schema": SCHEMA,
        "version": VERSION,
        "source": "google/fonts",
        "commit": commit,
        "base_url": format!("https://cdn.jsdelivr.net/gh/google/fonts@{commit}/"),
        "fallback_base_url": format!("https://raw.githubusercontent.com/google/fonts/{commit}/"),
        "languages": languages,
        "scripts": scripts,
        "families": families,
    }))
}

fn strings(msg: &pb::Msg, key: &str) -> Vec<String> {
    msg.all(key)
        .filter_map(Val::as_scalar)
        .map(str::to_string)
        .collect()
}

fn family_entry(
    repo: &Path,
    dir: &str,
    name: &str,
    lang: &LangData,
    warnings: &mut Vec<String>,
) -> Result<Option<Value>, String> {
    let fam_dir = repo.join(dir).join(name);
    let md = fam_dir.join("METADATA.pb");
    match std::fs::symlink_metadata(&md) {
        Ok(m) if m.is_file() => {}
        _ => return Ok(None),
    }
    let bytes = std::fs::read(&md).map_err(|e| format!("read METADATA.pb: {e}"))?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("METADATA.pb is too large".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    let msg = parse_pb(&text).map_err(|e| format!("malformed METADATA.pb: {e}"))?;
    let family = msg
        .str("name")
        .ok_or("METADATA.pb has no name")?
        .to_string();
    let license = msg
        .str("license")
        .map_or_else(|| dir.to_uppercase(), str::to_string);
    let designer = msg.str("designer").unwrap_or("").to_string();
    let categories = strings(&msg, "category");
    let mut subsets = strings(&msg, "subsets");
    subsets.sort();
    subsets.dedup();
    let license_path = LICENSE_FILES
        .iter()
        .find(|l| std::fs::symlink_metadata(fam_dir.join(l)).is_ok_and(|m| m.is_file()))
        .map(|l| format!("{dir}/{name}/{l}"));

    let mut cmap = BTreeSet::new();
    let mut files = Vec::new();
    for f in msg.all("fonts").filter_map(Val::as_msg) {
        let Some(filename) = f.str("filename") else {
            warnings.push(format!("{dir}/{name}: font entry without filename"));
            continue;
        };
        if filename.contains('/')
            || filename.contains('\\')
            || filename.contains("..")
            || filename.is_empty()
        {
            warnings.push(format!("{dir}/{name}: unsafe filename `{filename}`"));
            continue;
        }
        let path = fam_dir.join(filename);
        let Ok(m) = std::fs::symlink_metadata(&path) else {
            warnings.push(format!("{dir}/{name}: missing font file `{filename}`"));
            continue;
        };
        if !m.is_file() || m.len() == 0 || m.len() > MAX_FONT_BYTES {
            warnings.push(format!(
                "{dir}/{name}: `{filename}` is not a regular file of acceptable size"
            ));
            continue;
        }
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                warnings.push(format!("{dir}/{name}: read `{filename}`: {e}"));
                continue;
            }
        };
        let info = font::read_font(&data);
        // Clients parse every file before installing it, so an unparseable one is useless.
        if info.axes.is_none() {
            warnings.push(format!(
                "{dir}/{name}: `{filename}` does not parse as a font"
            ));
            continue;
        }
        cmap.extend(&info.codepoints);
        let weight = f.num("weight").map_or(400, |w| w.clamp(1.0, 1000.0) as u32);
        let mut ps: Vec<String> = Vec::new();
        ps.extend(f.str("post_script_name").map(str::to_string));
        ps.extend(info.postscript);
        ps.sort();
        ps.dedup();
        let axes: Vec<Value> = info
            .axes
            .unwrap_or_default()
            .into_iter()
            .map(|(t, lo, hi)| json!({"tag": t, "min": lo, "max": hi}))
            .collect();
        files.push(json!({
            "path": format!("{dir}/{name}/{filename}"),
            "size": data.len(),
            "sha256": sha256_hex(&data),
            "style": f.str("style").unwrap_or("normal"),
            "weight": weight,
            "axes": axes,
            "postscript": ps,
        }));
    }
    if files.is_empty() {
        return Err("no usable font files".into());
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));

    // Declared languages plus the ones the font's code points cover.
    let mut languages: BTreeSet<String> = strings(&msg, "languages").into_iter().collect();
    for l in &languages {
        if !lang.languages.iter().any(|k| k.id == *l) {
            warnings.push(format!(
                "{dir}/{name}: language `{l}` is not in the lang data"
            ));
        }
    }
    languages.extend(lang.covered(&cmap).into_iter().map(|l| l.id.clone()));
    let mut scripts: BTreeSet<String> = languages
        .iter()
        .filter_map(|id| lang.script_of(id))
        .collect();
    scripts.extend(
        msg.str("primary_script")
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    );
    for s in &subsets {
        match lang.subset_script(s) {
            Some(code) => {
                scripts.insert(code.to_string());
            }
            None if s == "menu" => {}
            None => warnings.push(format!("{dir}/{name}: subset `{s}` has no known script")),
        }
    }

    Ok(Some(json!({
        "family": family,
        "designer": designer,
        "categories": categories,
        "subsets": subsets,
        "languages": languages,
        "scripts": scripts,
        "license": license,
        "license_path": license_path,
        "files": files,
    })))
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
