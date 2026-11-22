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

/// An index's `version` as (major, minor): a `"MAJOR.MINOR"` string of decimal numbers.
fn parse_version(v: &Value) -> Option<(u64, u64)> {
    let (major, minor) = v.as_str()?.split_once('.')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(major) && digits(minor)).then_some((major.parse().ok()?, minor.parse().ok()?))
}

pub fn build(entries: &[Entry]) -> Result<Value, String> {
    let mut indexes = Vec::new();
    // The indexes' common version: `sources.json` says what its indexes say, which is the
    // generator's version only for indexes it wrote.
    let mut version: Option<Value> = None;
    for e in entries {
        let idx: Value =
            serde_json::from_slice(e.json).map_err(|err| format!("{}: {err}", e.path))?;
        // Its own version: a `MAJOR.MINOR` of this schema, no newer than the generator's.
        let own = parse_version(&idx["version"])
            .ok_or_else(|| format!("{}: version {} is not MAJOR.MINOR", e.path, idx["version"]))?;
        let mine = parse_version(&json!(VERSION)).expect("VERSION is MAJOR.MINOR");
        if own.0 != u64::from(SCHEMA) || own > mine {
            return Err(format!(
                "{}: version {} is not one this generator reads (schema {SCHEMA}, at most {VERSION})",
                e.path, idx["version"]
            ));
        }
        match &version {
            None => version = Some(idx["version"].clone()),
            Some(v) if *v != idx["version"] => {
                return Err(format!(
                    "{}: version {} differs from {v}; regenerate every index together",
                    e.path, idx["version"]
                ));
            }
            Some(_) => {}
        }
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
    Ok(
        json!({"schema": SCHEMA, "version": version.unwrap_or_else(|| json!(VERSION)), "indexes": indexes}),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn index(version: &str) -> Vec<u8> {
        json!({"version": version, "source": "s", "commit": "c", "source_info": {}, "families": []})
            .to_string()
            .into_bytes()
    }

    #[test]
    fn sources_json_says_its_indexes_version_and_refuses_a_mix() {
        let (a, b, c) = (index("1.2"), index("1.2"), index("1.3"));
        let entry = |path, json| Entry {
            path,
            json,
            gz: b"",
        };
        let same = build(&[entry("a.json", &a), entry("b.json", &b)]).unwrap();
        assert_eq!(same["version"], "1.2");
        let err = build(&[entry("a.json", &a), entry("c.json", &c)]).unwrap_err();
        assert!(err.contains("differs"), "{err}");
    }

    #[test]
    fn sources_json_refuses_a_version_that_is_not_one_it_reads() {
        let entry = |json: &'static [u8]| Entry {
            path: "a.json",
            json,
            gz: b"",
        };
        for (version, why) in [
            ("1.99", "not one this generator reads"),
            ("2.0", "not one this generator reads"),
            ("0.9", "not one this generator reads"),
            ("1", "not MAJOR.MINOR"),
            ("1.x", "not MAJOR.MINOR"),
            ("", "not MAJOR.MINOR"),
        ] {
            let json = index(version);
            let err = build(&[entry(Box::leak(json.into_boxed_slice()))]).unwrap_err();
            assert!(err.contains(why), "{version}: {err}");
        }
        let number = json!({"version": 1.2, "source": "s", "families": []}).to_string();
        let err = build(&[entry(Box::leak(number.into_bytes().into_boxed_slice()))]).unwrap_err();
        assert!(err.contains("not MAJOR.MINOR"), "{err}");
        assert!(build(&[entry(Box::leak(index(crate::VERSION).into_boxed_slice()))]).is_ok());
    }
}
