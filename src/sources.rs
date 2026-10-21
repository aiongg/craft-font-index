//! `sources.json`: the built-in indexes, each with its sha256, so a client that pins this one
//! file can check every index it downloads.

use serde_json::{Value, json};

use crate::{SCHEMA, VERSION, sha256_hex};

/// One built-in index: its path relative to `sources.json`, and its JSON and gzipped bytes.
pub struct Entry<'a> {
    pub path: &'a str,
    pub json: &'a [u8],
    pub gz: &'a [u8],
}

pub fn build(entries: &[Entry]) -> Result<Value, String> {
    let mut indexes = Vec::new();
    for e in entries {
        let idx: Value =
            serde_json::from_slice(e.json).map_err(|err| format!("{}: {err}", e.path))?;
        let info = &idx["source_info"];
        let families = idx["families"].as_array().map_or(&[][..], Vec::as_slice);
        let files: usize = families
            .iter()
            .map(|f| f["files"].as_array().map_or(0, Vec::len))
            .sum();
        indexes.push(json!({
            "source": idx["source"],
            "name": info["name"],
            "curated": info["curated"],
            "path": e.path,
            "size": e.json.len(),
            "sha256": sha256_hex(e.json),
            "gz_path": format!("{}.gz", e.path),
            "gz_size": e.gz.len(),
            "gz_sha256": sha256_hex(e.gz),
            "commit": idx["commit"],
            "families": families.len(),
            "files": files,
        }));
    }
    Ok(json!({"schema": SCHEMA, "version": VERSION, "indexes": indexes}))
}

/// `sources.json` with one index per line.
pub fn to_json(sources: &Value) -> String {
    let mut out = format!(
        "{{\"schema\":{},\"version\":{},\"indexes\":[\n",
        sources["schema"], sources["version"]
    );
    let list = sources["indexes"].as_array().map_or(&[][..], Vec::as_slice);
    for (i, e) in list.iter().enumerate() {
        out.push_str(&e.to_string());
        out.push_str(if i + 1 < list.len() { ",\n" } else { "\n" });
    }
    out.push_str("]}\n");
    out
}
