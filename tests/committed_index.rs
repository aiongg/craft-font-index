//! The committed indexes and `sources.json` follow the format in README.md. The Google Fonts
//! index also keeps every format 1.0 rule, including the ones photocraft#2453's reader enforces
//! (it rejects the whole index on any bad entry), so 1.0 readers still read it.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use craft_font_index as cfi;
use serde_json::Value;

const INDEXES: &[&str] = &["indexes/google-fonts.json", "indexes/noto.json"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn load(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(root().join(path)).unwrap()).unwrap()
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

/// The rules every index follows, whatever its source.
fn check_index(idx: &Value, licenses: &[&str]) {
    let version = idx["version"].as_str().unwrap();
    let major: u64 = version.split('.').next().unwrap().parse().unwrap();
    assert_eq!(major, 1);
    assert_eq!(idx["schema"].as_u64(), Some(major));
    assert_eq!(version, cfi::VERSION);
    let source = idx["source"].as_str().unwrap();
    let info = &idx["source_info"];
    assert_eq!(info["id"], source);
    assert!(!info["name"].as_str().unwrap().is_empty());
    assert!(info["homepage"].as_str().unwrap().starts_with("https://"));
    assert_eq!(info["curated"], true, "built-in indexes are curated");
    let templates = strs(&info["file_urls"]);
    assert!(!templates.is_empty());
    assert!(
        templates
            .iter()
            .all(|t| t.starts_with("https://") && t.contains("{path}"))
    );
    assert!(cfi::is_repo(idx["repo"].as_str().unwrap()));
    assert!(is_hex(idx["commit"].as_str().unwrap(), 40));
    assert_eq!(idx["ref"], idx["commit"]);

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

    let samples = idx["samples"].as_object().unwrap();
    for (script, sample) in samples {
        assert!(
            script.len() == 4 && script.is_ascii(),
            "sample script {script}"
        );
        let text = sample["text"].as_str().unwrap();
        assert!(
            !text.trim().is_empty() && text.chars().count() <= 60,
            "{script}"
        );
        let method = sample["method"].as_str().unwrap();
        assert!(["udhr", "iroha", "exemplar"].contains(&method), "{script}");
        assert!(!sample["language"].as_str().unwrap().is_empty(), "{script}");
    }

    let families = idx["families"].as_array().unwrap();
    let mut names = BTreeSet::new();
    let mut urls = BTreeSet::new();
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
        assert_eq!(fam["source"], source, "{name}");
        assert!(
            licenses.contains(&fam["license"].as_str().unwrap()),
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
        if let Some(r) = fam.get("repo") {
            assert!(cfi::is_repo(r.as_str().unwrap()), "{name}");
            assert!(cfi::is_ref(fam["ref"].as_str().unwrap()), "{name}");
        }
        let lf = &fam["license_file"];
        if !lf.is_null() {
            assert!(is_hex(lf["sha256"].as_str().unwrap(), 64), "{name}");
            assert!(lf["size"].as_u64().unwrap() > 0, "{name}");
            cfi::file_urls(idx, fam, lf).unwrap();
        }
        // 1.2 fields.
        let tags = strs(&fam["tags"]);
        assert!(!tags.is_empty(), "{name}: no tags");
        assert!(
            tags.windows(2).all(|w| w[0] < w[1]),
            "{name}: tags are not sorted and unique"
        );
        for t in &tags {
            assert!(cfi::tags::VOCABULARY.contains(t), "{name}: unknown tag {t}");
        }
        for r in strs(&fam["reserved_names"]) {
            assert!(!r.trim().is_empty(), "{name}: empty reserved name");
        }
        assert!(fam["reserved_names"].is_array(), "{name}");
        if let Some(c) = fam.get("copyright") {
            assert!(!c.as_str().unwrap().trim().is_empty(), "{name}");
        }
        if let Some(u) = fam.get("upstream") {
            assert!(u.as_str().unwrap().starts_with("https://"), "{name}");
        }
        if let Some(nf) = fam.get("notice_file") {
            assert!(is_hex(nf["sha256"].as_str().unwrap(), 64), "{name}");
            assert!(nf["size"].as_u64().unwrap() > 0, "{name}");
            cfi::file_urls(idx, fam, nf).unwrap();
        }

        let files = fam["files"].as_array().unwrap();
        assert!(!files.is_empty() && files.len() <= 1024);
        let mut faces = BTreeSet::new();
        // Clients install a family's files and licence side by side in one folder.
        let mut file_names = BTreeSet::new();
        if let Some(p) = lf["path"].as_str() {
            file_names.insert(p.rsplit('/').next().unwrap().to_lowercase());
        }
        for f in files {
            let path = f["path"].as_str().unwrap();
            assert!(
                file_names.insert(path.rsplit('/').next().unwrap().to_lowercase()),
                "{name}: two files named like {path}"
            );
            let file_urls = cfi::file_urls(idx, fam, f).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(file_urls.len(), templates.len(), "{path}");
            assert!(urls.insert(file_urls[0].clone()), "{path} is listed twice");
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
            let style = f["style"].as_str().unwrap();
            assert!(["normal", "italic"].contains(&style), "{path}");
            let width = f.get("width").map_or(100.0, |w| w.as_f64().unwrap());
            if f.get("width").is_some() {
                assert!(width > 0.0 && width <= 1000.0 && width != 100.0, "{path}");
            }
            for a in f["axes"].as_array().unwrap() {
                let tag = a["tag"].as_str().unwrap();
                assert!(tag.len() == 4 && tag.is_ascii(), "{path}");
                assert!(
                    a["min"].as_f64().unwrap() <= a["max"].as_f64().unwrap(),
                    "{path}"
                );
            }
            if f["axes"].as_array().unwrap().is_empty() {
                assert!(
                    faces.insert((style, weight, width.to_bits())),
                    "{name}: two static files for {style} {weight} {width}"
                );
            }
            // Every file shows a sample: its own text, or its script's (the family's unless the
            // file names its own).
            let script = f
                .get("sample_script")
                .unwrap_or(&fam["sample_script"])
                .as_str()
                .unwrap_or_else(|| panic!("{path}: no sample script"));
            match f.get("sample_text") {
                Some(t) => {
                    let t = t.as_str().unwrap();
                    assert!(!t.is_empty() && t.chars().count() <= 60, "{path}");
                }
                None => assert!(
                    samples.contains_key(script),
                    "{path}: no sample for {script}"
                ),
            }
            let instances = f
                .get("instances")
                .map_or(&[][..], |i| i.as_array().unwrap());
            let axes: Vec<&str> = f["axes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["tag"].as_str().unwrap())
                .collect();
            for i in instances {
                let coords = i["coordinates"].as_object().unwrap();
                assert!(i["name"].is_string(), "{path}");
                assert_eq!(
                    coords.keys().map(String::as_str).collect::<Vec<_>>(),
                    axes,
                    "{path}"
                );
            }
            if let Some(li) = f.get("list_instance") {
                let li = li.as_u64().unwrap() as usize;
                assert!(li < instances.len(), "{path}");
                assert!(
                    ["Regular", "Italic"].contains(&instances[li]["name"].as_str().unwrap()),
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
    }
}

#[test]
fn google_fonts_index_is_valid() {
    let idx = load("indexes/google-fonts.json");
    check_index(&idx, &["OFL", "APACHE2", "UFL"]);
    assert_eq!(idx["source"], cfi::google::SOURCE_ID);
    let families = idx["families"].as_array().unwrap();
    assert!(families.len() > 1000, "{} families", families.len());

    // Format 1.0: every file is base_url + path, in one licence/family directory.
    let commit = idx["commit"].as_str().unwrap();
    assert_eq!(
        idx["base_url"],
        format!("https://cdn.jsdelivr.net/gh/google/fonts@{commit}/")
    );
    assert_eq!(
        idx["fallback_base_url"],
        format!("https://raw.githubusercontent.com/google/fonts/{commit}/")
    );
    let mut old = serde_json::Map::new();
    for k in ["base_url", "fallback_base_url"] {
        old.insert(k.into(), idx[k].clone());
    }
    let old = Value::Object(old);
    for fam in families {
        let name = fam["family"].as_str().unwrap();
        assert!(
            fam.get("repo").is_none(),
            "{name}: every file is in google/fonts"
        );
        let mut dir: Option<String> = None;
        for f in fam["files"].as_array().unwrap() {
            let path = f["path"].as_str().unwrap();
            let parts: Vec<&str> = path.split('/').collect();
            assert_eq!(parts.len(), 3, "{path}");
            assert!(cfi::LICENSE_DIRS.contains(&parts[0]), "{path}");
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
            // The 1.1 templates give the same URLs as 1.0's base URLs.
            assert_eq!(
                cfi::file_urls(&idx, fam, f),
                cfi::file_urls(&old, fam, f),
                "{path}"
            );
        }
        match fam["license_path"].as_str() {
            Some(lp) => {
                assert!(
                    lp.starts_with(&format!("{}/", dir.unwrap())),
                    "{name}: {lp}"
                );
                assert_eq!(fam["license_file"]["path"], lp, "{name}");
            }
            None => assert!(fam["license_file"].is_null(), "{name}"),
        }
    }
}

#[test]
fn noto_index_is_valid() {
    let idx = load("indexes/noto.json");
    check_index(&idx, &["OFL"]);
    assert_eq!(idx["source"], cfi::noto::SOURCE_ID);
    assert!(
        idx.get("base_url").is_none(),
        "Noto files are in several repositories"
    );
    let google = load("indexes/google-fonts.json");
    let families = idx["families"].as_array().unwrap();
    for fam in families {
        let name = fam["family"].as_str().unwrap();
        assert!(
            fam["license_file"].is_object(),
            "{name}: every licence has a sha256"
        );
        let gf = google["families"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["family"].as_str().unwrap().eq_ignore_ascii_case(name));
        if let Some(g) = gf {
            assert!(
                cfi::noto::adds_styles(
                    fam["files"].as_array().unwrap(),
                    g["files"].as_array().unwrap()
                ),
                "{name}: Google Fonts has every style already"
            );
        }
    }
    for region in ["JP", "KR", "SC", "TC", "HK"] {
        for kind in ["Sans", "Serif"] {
            let name = format!("Noto {kind} CJK {region}");
            let fam = families
                .iter()
                .find(|f| f["family"] == name.as_str())
                .unwrap_or_else(|| panic!("{name} is missing"));
            assert_eq!(fam["repo"], cfi::noto::CJK_REPO);
            let weights: BTreeSet<u64> = fam["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["weight"].as_u64().unwrap())
                .collect();
            assert_eq!(weights.len(), 7, "{name}: {weights:?}");
        }
    }
}

#[test]
fn sources_json_pins_every_index() {
    let sources = load("sources.json");
    assert_eq!(sources["version"], cfi::VERSION);
    let list = sources["indexes"].as_array().unwrap();
    assert_eq!(list.len(), INDEXES.len());
    for (entry, path) in list.iter().zip(INDEXES) {
        assert_eq!(entry["path"], *path);
        let json = std::fs::read(root().join(path)).unwrap();
        let gz = std::fs::read(root().join(format!("{path}.gz"))).unwrap();
        assert_eq!(entry["sha256"], cfi::sha256_hex(&json), "{path}");
        assert_eq!(entry["size"], json.len(), "{path}");
        assert_eq!(entry["gz_sha256"], cfi::sha256_hex(&gz), "{path}");
        let idx: Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(entry["source"], idx["source"]);
        assert_eq!(entry["commit"], idx["commit"]);
        assert_eq!(entry["families"], idx["families"].as_array().unwrap().len());
    }
    let text = std::fs::read_to_string(root().join("sources.json")).unwrap();
    assert_eq!(cfi::sources::to_json(&sources), text, "sources.json layout");
}

#[test]
fn gzipped_copies_match() {
    for path in INDEXES {
        let json = std::fs::read(root().join(path)).unwrap();
        let gz = std::fs::read(root().join(format!("{path}.gz"))).unwrap();
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut out)
            .unwrap();
        assert!(out == json, "{path}.gz is not a copy of {path}");
        assert_eq!(
            gz,
            cfi::gzip(&json),
            "{path}: the gzipped copy is reproducible"
        );
    }
}

#[test]
fn layout_round_trips() {
    for path in INDEXES {
        let json = std::fs::read_to_string(root().join(path)).unwrap();
        let idx: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(cfi::to_json(&idx), json, "{path}");
    }
}

#[test]
fn file_urls_resolve_per_family_and_encode_paths() {
    let idx: Value = serde_json::json!({
        "source_info": {"file_urls": [
            "https://cdn.jsdelivr.net/gh/{repo}@{ref}/{path}",
            "https://raw.githubusercontent.com/{repo}/{ref}/{path}"
        ]},
        "repo": "a/b", "ref": "c1"
    });
    let plain = serde_json::json!({});
    let own = serde_json::json!({"repo": "x/y", "ref": "T2"});
    let file = serde_json::json!({"path": "d/Roboto[wdth,wght].ttf"});
    assert_eq!(
        cfi::file_urls(&idx, &plain, &file).unwrap(),
        [
            "https://cdn.jsdelivr.net/gh/a/b@c1/d/Roboto%5Bwdth%2Cwght%5D.ttf",
            "https://raw.githubusercontent.com/a/b/c1/d/Roboto%5Bwdth%2Cwght%5D.ttf"
        ]
    );
    assert!(
        cfi::file_urls(&idx, &own, &file).unwrap()[0]
            .starts_with("https://cdn.jsdelivr.net/gh/x/y@T2/")
    );
    let licence = serde_json::json!({"path": "OFL.txt", "repo": "n/m", "ref": "v1"});
    assert_eq!(
        cfi::file_urls(&idx, &own, &licence).unwrap()[1],
        "https://raw.githubusercontent.com/n/m/v1/OFL.txt"
    );
    for bad in ["../x.ttf", "/x.ttf", "a//b.ttf"] {
        assert!(
            cfi::file_urls(&idx, &plain, &serde_json::json!({"path": bad})).is_err(),
            "{bad}"
        );
    }
    let bad_ref = serde_json::json!({"repo": "x/y", "ref": "a/../b"});
    assert!(cfi::file_urls(&idx, &bad_ref, &file).is_err());
}
