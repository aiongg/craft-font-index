//! Checks the committed indexes against the data they were generated from. Ignored by default
//! because the checkouts take several GB; `scripts/regenerate.sh` runs them, or:
//!
//!     GOOGLE_FONTS_CHECKOUT=<dir> NOTO_MIRROR=<dir> cargo test --release -- --ignored
//!
//! `downloads_match_index` fetches a few files over the network and needs neither.

use std::path::{Path, PathBuf};
use std::process::Command;

use craft_font_index as cfi;
use serde_json::Value;

/// Static and variable, italic, declared languages, each licence directory.
const GOOGLE_SAMPLE: &[&str] = &[
    "ofl/lato",
    "ofl/roboto",
    "ofl/notosansjp",
    "ofl/notosansarabic",
    "apache/kosugi",
    "ufl/ubuntu",
];

/// CJK, a variable font, static widths, and a sub-family of a state.json family.
const NOTO_SAMPLE: &[&str] = &[
    "Noto Sans CJK JP",
    "Noto Serif Myanmar",
    "Noto Sans Gujarati UI",
    "Noto Sans Tifinagh Ghat",
];

fn committed(path: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn git_head(dir: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn env_dir(var: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(var).unwrap_or_else(|| panic!("set {var}")))
}

/// The google/fonts checkout, after checking it is at the index's commit.
fn google_checkout(idx: &Value) -> PathBuf {
    let dir = env_dir("GOOGLE_FONTS_CHECKOUT");
    assert_eq!(
        git_head(&dir),
        idx["commit"].as_str().unwrap(),
        "the checkout is not at the index's commit"
    );
    dir
}

/// The Noto mirror, after checking it is at the index's commit and tags.
fn noto_mirror(idx: &Value) -> PathBuf {
    let dir = env_dir("NOTO_MIRROR");
    assert_eq!(
        git_head(&dir.join("github.io")),
        idx["commit"].as_str().unwrap(),
        "the mirror is not at the index's commit"
    );
    let m = cfi::noto::Manifest::load(&dir).unwrap();
    for fam in idx["families"].as_array().unwrap() {
        if let Some(r) = fam["ref"].as_str() {
            assert!(
                [&m.cjk_sans_tag, &m.cjk_serif_tag].contains(&&r.to_string()),
                "{r}"
            );
        }
    }
    dir
}

/// A family with its language positions replaced by IDs, so entries from indexes with
/// different language tables compare.
fn resolved(idx: &Value, fam: &Value) -> Value {
    let mut fam = fam.clone();
    let ids: Vec<Value> = fam["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| idx["languages"][i.as_u64().unwrap() as usize]["id"].clone())
        .collect();
    fam["languages"] = Value::Array(ids);
    fam
}

fn family_dir(fam: &Value) -> String {
    let path = fam["files"][0]["path"].as_str().unwrap();
    path.rsplit_once('/').unwrap().0.to_string()
}

fn files(idx: &Value) -> Vec<(&Value, &Value)> {
    idx["families"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|fam| {
            fam["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |f| (fam, f))
        })
        .collect()
}

fn check_file(local: &Path, f: &Value) {
    let data = std::fs::read(local).unwrap_or_else(|e| panic!("{}: {e}", local.display()));
    assert_eq!(
        data.len() as u64,
        f["size"].as_u64().unwrap(),
        "{}",
        local.display()
    );
    assert_eq!(
        cfi::sha256_hex(&data),
        f["sha256"].as_str().unwrap(),
        "{}",
        local.display()
    );
}

#[test]
#[ignore = "needs GOOGLE_FONTS_CHECKOUT"]
fn regenerating_google_sample_matches() {
    let idx = committed("indexes/google-fonts.json");
    let repo = google_checkout(&idx);
    let only: Vec<String> = GOOGLE_SAMPLE.iter().map(|s| s.to_string()).collect();
    let commit = idx["commit"].as_str().unwrap();
    let fresh = cfi::build_index(&repo, commit, Some(&only), &mut Vec::new()).unwrap();
    let fresh_fams = fresh["families"].as_array().unwrap();
    assert_eq!(fresh_fams.len(), GOOGLE_SAMPLE.len());
    for fam in fresh_fams {
        let dir = family_dir(fam);
        let old = idx["families"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| family_dir(f) == dir)
            .unwrap_or_else(|| panic!("{dir} is not in the committed index"));
        assert_eq!(resolved(&fresh, fam), resolved(&idx, old), "{dir}");
    }
}

#[test]
#[ignore = "needs GOOGLE_FONTS_CHECKOUT"]
fn google_sample_files_match_sha256_and_size() {
    let idx = committed("indexes/google-fonts.json");
    let repo = google_checkout(&idx);
    let all = files(&idx);
    // Every 20th file, plus the last one.
    let sample: Vec<_> = all.iter().step_by(20).chain(all.last()).collect();
    for (_, f) in &sample {
        check_file(&repo.join(f["path"].as_str().unwrap()), f);
    }
    assert!(sample.len() > 100);
}

#[test]
#[ignore = "needs GOOGLE_FONTS_CHECKOUT and NOTO_MIRROR"]
fn regenerating_noto_sample_matches() {
    let idx = committed("indexes/noto.json");
    let mirror = noto_mirror(&idx);
    let google = committed("indexes/google-fonts.json");
    let lang = cfi::lang::LangData::load(&google_checkout(&google), &mut Vec::new()).unwrap();
    let only: Vec<String> = NOTO_SAMPLE.iter().map(|s| s.to_string()).collect();
    let (fresh, _) =
        cfi::noto::build_index(&mirror, &lang, &google, Some(&only), &mut Vec::new()).unwrap();
    let fresh_fams = fresh["families"].as_array().unwrap();
    assert_eq!(fresh_fams.len(), NOTO_SAMPLE.len(), "{fresh_fams:?}");
    for fam in fresh_fams {
        let name = fam["family"].as_str().unwrap();
        let old = idx["families"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["family"] == name)
            .unwrap_or_else(|| panic!("{name} is not in the committed index"));
        assert_eq!(resolved(&fresh, fam), resolved(&idx, old), "{name}");
    }
}

#[test]
#[ignore = "needs NOTO_MIRROR"]
fn noto_sample_files_match_sha256_and_size() {
    let idx = committed("indexes/noto.json");
    let mirror = noto_mirror(&idx);
    let local = |fam: &Value, path: &str| match fam["ref"].as_str() {
        Some(r) if r.starts_with("Sans") => mirror.join("noto-cjk-sans").join(path),
        Some(_) => mirror.join("noto-cjk-serif").join(path),
        None => mirror.join("github.io").join(path),
    };
    let all = files(&idx);
    // Every 5th file, plus the last one.
    let sample: Vec<_> = all.iter().step_by(5).chain(all.last()).collect();
    for (fam, f) in &sample {
        check_file(&local(fam, f["path"].as_str().unwrap()), f);
    }
    assert!(sample.len() > 20);
    for fam in idx["families"].as_array().unwrap() {
        let lf = &fam["license_file"];
        let path = lf["path"].as_str().unwrap();
        let file = match lf["repo"].as_str() {
            Some(repo) => mirror
                .join("licenses")
                .join(repo.strip_prefix("notofonts/").unwrap())
                .join(lf["ref"].as_str().unwrap())
                .join(path),
            None => local(fam, path),
        };
        check_file(&file, lf);
    }
}

fn curl(url: &str) -> Option<Vec<u8>> {
    let out = Command::new("curl")
        .args(["-sSfL", "--max-time", "120", url])
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

#[test]
#[ignore = "downloads from jsDelivr and GitHub"]
fn downloads_match_index() {
    for path in ["indexes/google-fonts.json", "indexes/noto.json"] {
        let idx = committed(path);
        let mut all = files(&idx);
        all.sort_by_key(|(_, f)| f["size"].as_u64().unwrap());
        // The three smallest files and one licence; every URL of each is checked.
        let fam = all[0].0;
        let mut entries: Vec<(&Value, &Value)> = all.into_iter().take(3).collect();
        entries.push((fam, &fam["license_file"]));
        for (fam, f) in entries {
            for url in cfi::file_urls(&idx, fam, f).unwrap() {
                let data = curl(&url).unwrap_or_else(|| panic!("{url} failed"));
                assert_eq!(
                    cfi::sha256_hex(&data),
                    f["sha256"].as_str().unwrap(),
                    "{url}"
                );
            }
        }
    }
}
