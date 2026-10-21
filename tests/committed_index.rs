//! The committed index follows the format in README.md, including the rules photocraft#2453's
//! reader enforces (it rejects the whole index on any bad entry).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect()
}

#[test]
fn committed_index_is_valid() {
    let bytes = std::fs::read(root().join("google-fonts-index.json")).unwrap();
    let idx: Value = serde_json::from_slice(&bytes).unwrap();

    let version = idx["version"].as_str().unwrap();
    let major: u64 = version.split('.').next().unwrap().parse().unwrap();
    assert_eq!(major, 1);
    assert_eq!(idx["schema"].as_u64(), Some(major));
    assert_eq!(version, google_fonts_index::VERSION);
    let commit = idx["commit"].as_str().unwrap();
    assert!(is_hex(commit, 40), "{commit}");
    assert_eq!(
        idx["base_url"],
        format!("https://cdn.jsdelivr.net/gh/google/fonts@{commit}/")
    );
    assert_eq!(
        idx["fallback_base_url"],
        format!("https://raw.githubusercontent.com/google/fonts/{commit}/")
    );

    let languages = idx["languages"].as_array().unwrap();
    let lang_ids: Vec<&str> = languages
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert!(
        lang_ids.windows(2).all(|w| w[0] < w[1]),
        "language table is sorted and unique"
    );
    let script_ids: Vec<&str> = idx["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert!(
        script_ids.windows(2).all(|w| w[0] < w[1]),
        "script table is sorted and unique"
    );
    for l in languages {
        assert!(script_ids.contains(&l["script"].as_str().unwrap()), "{l}");
        assert!(!l["name"].as_str().unwrap().is_empty());
        assert!(l["population"].is_u64() && l["historical"].is_boolean());
    }

    let families = idx["families"].as_array().unwrap();
    assert!(families.len() > 1000, "{} families", families.len());
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut prev = String::new();
    for fam in families {
        let name = fam["family"].as_str().unwrap();
        assert!(!name.trim().is_empty() && name.len() <= 512);
        assert!(
            names.insert(name.to_lowercase()),
            "family `{name}` is listed twice"
        );
        assert!(
            prev <= name.to_lowercase(),
            "families are sorted by name: `{prev}` before `{name}`"
        );
        prev = name.to_lowercase();
        assert!(
            ["OFL", "APACHE2", "UFL"].contains(&fam["license"].as_str().unwrap()),
            "{name}"
        );
        assert!(fam["designer"].is_string());
        for s in strs(&fam["categories"])
            .into_iter()
            .chain(strs(&fam["subsets"]))
        {
            assert!(!s.is_empty() && s.len() <= 512);
        }
        for i in fam["languages"].as_array().unwrap() {
            assert!((i.as_u64().unwrap() as usize) < languages.len(), "{name}");
        }
        for s in strs(&fam["scripts"]) {
            assert!(script_ids.contains(&s), "{name}: script {s}");
        }

        let files = fam["files"].as_array().unwrap();
        assert!(!files.is_empty() && files.len() <= 1024);
        let mut dir: Option<String> = None;
        for f in files {
            let path = f["path"].as_str().unwrap();
            assert!(paths.insert(path), "{path} is listed twice");
            let parts: Vec<&str> = path.split('/').collect();
            assert_eq!(parts.len(), 3, "{path}");
            assert!(
                google_fonts_index::LICENSE_DIRS.contains(&parts[0]),
                "{path}"
            );
            assert!(
                !parts[1].is_empty()
                    && parts[1].len() <= 64
                    && parts[1].bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b == b'_'
                        || b == b'-'),
                "{path}"
            );
            let d = format!("{}/{}", parts[0], parts[1]);
            assert_eq!(
                dir.get_or_insert(d.clone()),
                &d,
                "{name}: files in one directory"
            );
            assert!(path.ends_with(".ttf") || path.ends_with(".otf"), "{path}");
            assert!(
                !path.contains("..") && !path.contains('\\') && !path.contains(':'),
                "{path}"
            );
            assert!(is_hex(f["sha256"].as_str().unwrap(), 64), "{path}");
            let size = f["size"].as_u64().unwrap();
            assert!(size > 0 && size <= 64 * 1024 * 1024, "{path}");
            let weight = f["weight"].as_u64().unwrap();
            assert!((1..=1000).contains(&weight), "{path}");
            assert!(
                ["normal", "italic"].contains(&f["style"].as_str().unwrap()),
                "{path}"
            );
            for a in f["axes"].as_array().unwrap() {
                let tag = a["tag"].as_str().unwrap();
                assert!(tag.len() == 4 && tag.is_ascii(), "{path}");
                assert!(
                    a["min"].as_f64().unwrap() <= a["max"].as_f64().unwrap(),
                    "{path}"
                );
            }
            let ps = strs(&f["postscript"]);
            assert!(!ps.is_empty(), "{path} has no PostScript name");
            assert!(
                ps.iter()
                    .all(|p| !p.is_empty() && p.len() <= 512 && !p.chars().any(char::is_control))
            );
        }
        if let Some(lp) = fam["license_path"].as_str() {
            assert!(
                lp.starts_with(&format!("{}/", dir.unwrap())),
                "{name}: {lp}"
            );
        }
    }
}

#[test]
fn gzipped_copy_matches() {
    let json = std::fs::read(root().join("google-fonts-index.json")).unwrap();
    let gz = std::fs::read(root().join("google-fonts-index.json.gz")).unwrap();
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_end(&mut out)
        .unwrap();
    assert!(
        out == json,
        "google-fonts-index.json.gz is not a copy of google-fonts-index.json"
    );
    assert_eq!(
        gz,
        google_fonts_index::gzip(&json),
        "the gzipped copy is reproducible"
    );
}

#[test]
fn layout_round_trips() {
    let json = std::fs::read_to_string(root().join("google-fonts-index.json")).unwrap();
    let idx: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(google_fonts_index::to_json(&idx), json);
}
