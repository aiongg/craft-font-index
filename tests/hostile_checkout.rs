//! A checkout with broken and hostile entries gives warnings, not an error, a panic or a file
//! outside the family directory.

use std::path::PathBuf;

fn tmp_repo() -> PathBuf {
    let p =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("hostile-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    for d in [
        "lang/Lib/gflanguages/data/languages",
        "lang/Lib/gflanguages/data/scripts",
    ] {
        std::fs::create_dir_all(p.join(d)).unwrap();
    }
    p
}

#[test]
fn hostile_entries_are_warnings() {
    let repo = tmp_repo();
    for (dir, md) in [
        ("ofl/broken", "name: \"Broken\" fonts {"),
        ("ofl/noname", "fonts { filename: \"a.ttf\" }\n"),
        (
            "apache/nofile",
            "name: \"No File\"\nfonts { filename: \"gone.ttf\" }\n",
        ),
        (
            "ufl/evil",
            "name: \"Evil\"\nfonts { filename: \"../../ofl/notafont/NotAFont.ttf\" }\n",
        ),
        (
            "ofl/notafont",
            "name: \"Not A Font\"\nfonts { filename: \"NotAFont.ttf\" }\n",
        ),
    ] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
        std::fs::write(repo.join(dir).join("METADATA.pb"), md).unwrap();
    }
    std::fs::write(repo.join("ofl/notafont/NotAFont.ttf"), b"not a font").unwrap();
    #[cfg(unix)]
    {
        std::fs::create_dir_all(repo.join("ofl/link")).unwrap();
        std::fs::write(
            repo.join("ofl/link/METADATA.pb"),
            "name: \"Link\"\nfonts { filename: \"L.ttf\" }\n",
        )
        .unwrap();
        std::os::unix::fs::symlink("/etc/passwd", repo.join("ofl/link/L.ttf")).unwrap();
    }

    let mut warnings = Vec::new();
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let idx = google_fonts_index::build_index(&repo, commit, None, &mut warnings).unwrap();
    assert_eq!(idx["families"].as_array().unwrap().len(), 0, "{idx}");
    let expect = [
        "malformed",
        "has no name",
        "missing font file",
        "unsafe filename",
        "does not parse",
    ];
    for e in expect {
        assert!(
            warnings.iter().any(|w| w.contains(e)),
            "no `{e}` warning in {warnings:?}"
        );
    }
    assert!(google_fonts_index::build_index(&repo, "../x", None, &mut Vec::new()).is_err());
    let _ = std::fs::remove_dir_all(&repo);
}
