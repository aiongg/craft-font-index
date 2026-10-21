//! Checks the committed index against a google/fonts checkout at the index's commit. Ignored by
//! default because the checkout is several GB; `scripts/regenerate.sh` runs them, or:
//!
//!     GOOGLE_FONTS_CHECKOUT=<dir> cargo test --release -- --ignored

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// Static and variable, italic, declared languages, each licence directory.
const SAMPLE_FAMILIES: &[&str] = &[
    "ofl/lato",
    "ofl/roboto",
    "ofl/notosansjp",
    "ofl/notosansarabic",
    "apache/kosugi",
    "ufl/ubuntu",
];

fn committed() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("google-fonts-index.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The checkout, after checking it is at the index's commit.
fn checkout(idx: &Value) -> PathBuf {
    let dir = PathBuf::from(
        std::env::var_os("GOOGLE_FONTS_CHECKOUT")
            .expect("set GOOGLE_FONTS_CHECKOUT to a google/fonts checkout"),
    );
    let out = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        head,
        idx["commit"].as_str().unwrap(),
        "the checkout is not at the index's commit"
    );
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

#[test]
#[ignore = "needs GOOGLE_FONTS_CHECKOUT"]
fn regenerating_sample_families_matches() {
    let idx = committed();
    let repo = checkout(&idx);
    let only: Vec<String> = SAMPLE_FAMILIES.iter().map(|s| s.to_string()).collect();
    let commit = idx["commit"].as_str().unwrap();
    let fresh =
        google_fonts_index::build_index(&repo, commit, Some(&only), &mut Vec::new()).unwrap();
    let fresh_fams = fresh["families"].as_array().unwrap();
    assert_eq!(fresh_fams.len(), SAMPLE_FAMILIES.len());
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
fn sample_files_match_sha256_and_size() {
    let idx = committed();
    let repo = checkout(&idx);
    let files: Vec<&Value> = idx["families"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["files"].as_array().unwrap())
        .collect();
    // Every 20th file, plus the last one.
    let sample: Vec<&Value> = files
        .iter()
        .step_by(20)
        .chain(files.last())
        .copied()
        .collect();
    for f in &sample {
        let path = f["path"].as_str().unwrap();
        let data = std::fs::read(repo.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(data.len() as u64, f["size"].as_u64().unwrap(), "{path}");
        assert_eq!(
            google_fonts_index::sha256_hex(&data),
            f["sha256"].as_str().unwrap(),
            "{path}"
        );
    }
    assert!(sample.len() > 100);
}
