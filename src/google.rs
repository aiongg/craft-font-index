//! The Google Fonts index, from a checkout of google/fonts.
//!
//! Walks `{ofl,apache,ufl}/*/METADATA.pb`, hashes every listed font file and reads axes,
//! PostScript names and code points from the font itself. Based on @kntrttm's
//! `xtask fonts-index` (photocraft#2453).

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

use crate::lang::LangData;
use crate::licence::{copyright_header, reserved_names};
use crate::pb::{self, Val, parse_pb};
use crate::sample;
use crate::tags::{Curation, GoogleTags};
use crate::{IndexHeader, MAX_FONT_BYTES, file_record, finish_index, is_commit, license_record};

pub const SOURCE_ID: &str = "google/fonts";
pub const REPO: &str = "google/fonts";
pub const LICENSE_DIRS: &[&str] = &["ofl", "apache", "ufl"];
const LICENSE_FILES: &[&str] = &["OFL.txt", "LICENSE.txt", "UFL.txt"];
const NOTICE_FILES: &[&str] = &["NOTICE", "NOTICE.txt", "NOTICE.md"];

/// Builds the index for the checkout at `repo`. With `only`, just the family directories named
/// there (`ofl/lato`, or `lato` in any licence directory). Per-family problems go to `warnings`.
pub fn build_index(
    repo: &Path,
    commit: &str,
    only: Option<&[String]>,
    warnings: &mut Vec<String>,
) -> Result<Value, String> {
    if !is_commit(commit) {
        return Err(format!("commit `{commit}` is not a full lowercase hex sha"));
    }
    let lang = LangData::load(repo, warnings)?;
    let google_tags = GoogleTags::load(repo, warnings);
    let curation = Curation::load_default()?;
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
            match family_entry(repo, d, &name, &lang, &google_tags, &curation, warnings) {
                Ok(Some(f)) => families.push(f),
                Ok(None) => {}
                Err(w) => warnings.push(format!("{d}/{name}: {w}")),
            }
        }
    }
    let header = IndexHeader {
        source: SOURCE_ID,
        name: "Google Fonts",
        homepage: "https://fonts.google.com",
        commit,
        repo: REPO,
        base_urls: Some((
            format!("https://cdn.jsdelivr.net/gh/{REPO}@{commit}/"),
            format!("https://raw.githubusercontent.com/{REPO}/{commit}/"),
        )),
    };
    Ok(finish_index(&header, families, &lang))
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
    google_tags: &GoogleTags,
    curation: &Curation,
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
    let license_file = match &license_path {
        Some(p) => license_record(&repo.join(p), p, None)
            .map_err(|e| warnings.push(format!("{dir}/{name}: {e}")))
            .ok(),
        None => None,
    };

    let notice_file = NOTICE_FILES
        .iter()
        .find(|l| std::fs::symlink_metadata(fam_dir.join(l)).is_ok_and(|m| m.is_file()))
        .and_then(|l| {
            let p = format!("{dir}/{name}/{l}");
            license_record(&repo.join(&p), &p, None)
                .map_err(|e| warnings.push(format!("{dir}/{name}: {e}")))
                .ok()
        });
    // METADATA.pb's per-font copyright, else the notice at the top of the licence file.
    let ofl_header = license_path.as_ref().and_then(|p| {
        let m = std::fs::symlink_metadata(repo.join(p)).ok()?;
        if m.len() > 1024 * 1024 {
            return None;
        }
        copyright_header(&String::from_utf8_lossy(&std::fs::read(repo.join(p)).ok()?))
    });
    let copyright = msg
        .all("fonts")
        .filter_map(Val::as_msg)
        .find_map(|f| f.str("copyright").filter(|c| !c.trim().is_empty()))
        // METADATA.pb strings can hold line breaks; the index joins the parts with spaces.
        .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
        .or_else(|| ofl_header.clone());
    let reserved = if license == "OFL" {
        let from_header = ofl_header
            .as_deref()
            .map(reserved_names)
            .unwrap_or_default();
        if from_header.is_empty() {
            copyright.as_deref().map(reserved_names).unwrap_or_default()
        } else {
            from_header
        }
    } else {
        Vec::new()
    };
    let upstream = msg
        .all("source")
        .filter_map(Val::as_msg)
        .find_map(|s| s.str("repository_url").filter(|u| !u.is_empty()))
        .map(str::to_string);
    let cats: Vec<&str> = categories.iter().map(String::as_str).collect();
    let tags: Vec<String> = match curation.get(&family) {
        Some(t) => t.to_vec(),
        None => google_tags.tags_for(&family, &cats),
    };

    let primary = msg.str("primary_script").filter(|s| !s.is_empty());
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
        let info = crate::font::read_font(&data);
        // Clients parse every file before installing it, so an unparseable one is useless.
        if info.axes.is_none() {
            warnings.push(format!(
                "{dir}/{name}: `{filename}` does not parse as a font"
            ));
            continue;
        }
        cmap.extend(&info.codepoints);
        let weight = f.num("weight").map_or(400, |w| w.clamp(1.0, 1000.0) as u32);
        let style = f.str("style").unwrap_or("normal");
        let extra_ps = f.str("post_script_name").map(str::to_string);
        let pick = sample::pick(&lang.samples, primary, &info.codepoints);
        let mut rec = file_record(
            format!("{dir}/{name}/{filename}"),
            &data,
            info,
            style,
            weight,
            extra_ps,
        );
        sample::apply(&mut rec, pick);
        files.push(rec);
    }
    if files.is_empty() {
        return Err("no usable font files".into());
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let sample_script = sample::settle(&mut files, primary);

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

    let mut fam = json!({
        "family": family,
        "source": SOURCE_ID,
        "designer": designer,
        "categories": categories,
        "subsets": subsets,
        "languages": languages,
        "scripts": scripts,
        "license": license,
        "license_path": license_path,
        "license_file": license_file,
        "tags": tags,
        "reserved_names": reserved,
    });
    if let Some(c) = copyright {
        fam["copyright"] = json!(c);
    }
    if let Some(n) = notice_file {
        fam["notice_file"] = n;
    }
    if let Some(u) = upstream {
        fam["upstream"] = json!(u);
    }
    fam["sample_script"] = json!(sample_script);
    fam["files"] = json!(files);
    Ok(Some(fam))
}
