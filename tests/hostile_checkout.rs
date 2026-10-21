//! A checkout with broken and hostile entries gives warnings, not an error, a panic or a file
//! outside the family directory.

use std::path::PathBuf;

fn tmp_repo(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("hostile-{name}-{}", std::process::id()));
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
    let repo = tmp_repo("google");
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
    let idx = craft_font_index::build_index(&repo, commit, None, &mut warnings).unwrap();
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
    assert!(craft_font_index::build_index(&repo, "../x", None, &mut Vec::new()).is_err());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn hostile_noto_mirror_entries_are_warnings() {
    let repo = tmp_repo("noto");
    let lang = craft_font_index::lang::LangData::load(&repo, &mut Vec::new()).unwrap();
    let mirror = repo.join("noto");
    let io = mirror.join("github.io");
    std::fs::create_dir_all(io.join("fonts/NotoSansX/full/ttf")).unwrap();
    std::fs::write(
        io.join("fonts/NotoSansX/full/ttf/NotoSansX-Regular.ttf"),
        b"not a font",
    )
    .unwrap();
    let release = |tag: &str| serde_json::json!({"url": format!("https://github.com/notofonts/x/releases/tag/{tag}")});
    let state = serde_json::json!({
        "x": {"families": {
            "Noto Sans X": {"files": ["fonts/NotoSansX/full/ttf/NotoSansX-Regular.ttf"],
                            "latest_release": release("NotoSansX-v1.000")},
            "Noto Sans Escape": {"files": ["fonts/../../../etc/full/ttf/passwd.ttf",
                                           "/etc/full/ttf/x/passwd.ttf"],
                                 "latest_release": release("v1")},
            "Noto Sans Bad Tag": {"files": [], "latest_release": release("..")},
            "Noto Sans No Release": {"files": []}
        }},
        "../evil": {"families": {"Evil": {"files": [], "latest_release": release("v1")}}}
    });
    std::fs::write(io.join("state.json"), state.to_string()).unwrap();
    for kind in ["sans", "serif"] {
        let root = mirror.join(format!("noto-cjk-{kind}"));
        for d in [
            "Sans/OTF/Japanese",
            "Sans/OTF/Korean",
            "Sans/OTF/SimplifiedChinese",
            "Sans/OTF/TraditionalChinese",
            "Sans/OTF/TraditionalChineseHK",
            "Sans/Mono",
            "Serif/OTF/Japanese",
            "Serif/OTF/Korean",
            "Serif/OTF/SimplifiedChinese",
            "Serif/OTF/TraditionalChinese",
            "Serif/OTF/TraditionalChineseHK",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
    }
    let manifest = serde_json::json!({
        "notofonts_commit": "0123456789abcdef0123456789abcdef01234567",
        "cjk_sans_tag": "Sans2.004", "cjk_serif_tag": "Serif2.003"
    });
    std::fs::write(mirror.join("manifest.json"), manifest.to_string()).unwrap();
    let google = serde_json::json!({"families": []});

    let mut warnings = Vec::new();
    let (idx, _) =
        craft_font_index::noto::build_index(&mirror, &lang, &google, None, &mut warnings).unwrap();
    assert_eq!(idx["families"].as_array().unwrap().len(), 0, "{idx}");
    for e in [
        "does not parse",
        "no TTF build",
        "unsafe release tag",
        "no release",
        "unsafe repository",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(e)),
            "no `{e}` warning in {warnings:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&repo);
}
