//! Family tags: taxonomy traits from google/fonts' `tags/all/families.csv`, with fallbacks
//! from the METADATA category and the family name, and overrides from `curation.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Tags whose Google score is below this are ignored.
pub const MIN_SCORE: f64 = 50.0;

/// Every tag the indexes use: `Group/Trait`, `Monospace`, and the bare groups of the fallback.
pub const VOCABULARY: &[&str] = &[
    "Sans",
    "Sans/Geometric",
    "Sans/Humanist",
    "Sans/Grotesque",
    "Sans/Rounded",
    "Serif",
    "Serif/Old style",
    "Serif/Transitional",
    "Serif/Modern",
    "Serif/Slab",
    "Script/Handwritten",
    "Script/Informal",
    "Script/Formal",
    "Script/Brush",
    "Display",
    "Display/Playful",
    "Display/Vintage",
    "Display/Futuristic",
    "Display/Blackletter",
    "Display/Pixel",
    "Display/Stencil",
    "Display/Distressed",
    "Feeling/Calm",
    "Feeling/Business",
    "Feeling/Elegant",
    "Feeling/Cute",
    "Feeling/Loud",
    "Feeling/Artistic",
    "Monospace",
];

/// The taxonomy tag for a Google tag path, if it has one.
pub fn map_path(path: &str) -> Option<&'static str> {
    if path.starts_with("/Slab/") {
        return Some("Serif/Slab");
    }
    Some(match path {
        "/Sans/Geometric" => "Sans/Geometric",
        "/Sans/Humanist" => "Sans/Humanist",
        "/Sans/Grotesque" | "/Sans/Neo Grotesque" => "Sans/Grotesque",
        "/Sans/Rounded" => "Sans/Rounded",
        "/Serif/Old Style Garalde" | "/Serif/Humanist Venetian" => "Serif/Old style",
        "/Serif/Transitional" => "Serif/Transitional",
        "/Serif/Modern" | "/Serif/Fat Face" => "Serif/Modern",
        "/Script/Handwritten" => "Script/Handwritten",
        "/Script/Informal" => "Script/Informal",
        "/Script/Formal" | "/Script/Upright Script" => "Script/Formal",
        "/Theme/Brush" => "Script/Brush",
        "/Expressive/Playful" | "/Theme/Wacky" => "Display/Playful",
        "/Expressive/Vintage" | "/Theme/Woodtype" | "/Theme/Art Nouveau" => "Display/Vintage",
        "/Expressive/Futuristic" | "/Theme/Techno" => "Display/Futuristic",
        "/Theme/Blackletter" => "Display/Blackletter",
        "/Theme/Pixel" => "Display/Pixel",
        "/Theme/Stencil" => "Display/Stencil",
        "/Theme/Distressed" => "Display/Distressed",
        "/Expressive/Calm" => "Feeling/Calm",
        "/Expressive/Business" => "Feeling/Business",
        "/Expressive/Sophisticated" | "/Expressive/Fancy" => "Feeling/Elegant",
        "/Expressive/Cute" => "Feeling/Cute",
        "/Expressive/Loud" => "Feeling/Loud",
        "/Expressive/Artistic" => "Feeling/Artistic",
        "/Monospace/Monospace" => "Monospace",
        _ => return None,
    })
}

/// The tag for a METADATA.pb `category`, used when a family has no mapped Google tag.
pub fn from_category(category: &str) -> Option<&'static str> {
    Some(match category {
        "SANS_SERIF" => "Sans",
        "SERIF" => "Serif",
        "DISPLAY" => "Display",
        "HANDWRITING" => "Script/Handwritten",
        "MONOSPACE" => "Monospace",
        _ => return None,
    })
}

/// Google's scored tags for each family.
#[derive(Default)]
pub struct GoogleTags(BTreeMap<String, Vec<(String, f64)>>);

impl GoogleTags {
    /// Reads `tags/all/families.csv` (no header; family, axes, tagpath, score) from a
    /// google/fonts checkout. A missing file gives no Google tags and a warning.
    pub fn load(repo: &Path, warnings: &mut Vec<String>) -> GoogleTags {
        let p = repo.join("tags/all/families.csv");
        match std::fs::read_to_string(&p) {
            Ok(text) => GoogleTags::parse(&text),
            Err(e) => {
                warnings.push(format!(
                    "{}: {e}; families get fallback tags only",
                    p.display()
                ));
                GoogleTags::default()
            }
        }
    }

    /// Rows with a non-empty axes column rate a variation of the family and are skipped.
    pub fn parse(csv: &str) -> GoogleTags {
        let mut m: BTreeMap<String, Vec<(String, f64)>> = BTreeMap::new();
        for line in csv.lines() {
            let f = csv_fields(line);
            let [family, axes, path, score] = f.as_slice() else {
                continue;
            };
            if !axes.is_empty() {
                continue;
            }
            let Ok(score) = score.trim().parse::<f64>() else {
                continue;
            };
            m.entry(family.clone())
                .or_default()
                .push((path.clone(), score));
        }
        GoogleTags(m)
    }

    /// Whether any mapped tag reaches the threshold for `family`.
    pub fn has_mapped(&self, family: &str) -> bool {
        self.0.get(family).is_some_and(|v| {
            v.iter()
                .any(|(p, s)| *s >= MIN_SCORE && map_path(p).is_some())
        })
    }

    /// The taxonomy tags of `family`, sorted: mapped Google tags scoring at least
    /// [`MIN_SCORE`], plus `Monospace` for a monospace category; when that leaves none, the
    /// category's own tag.
    pub fn tags_for(&self, family: &str, categories: &[&str]) -> Vec<String> {
        let mut set: BTreeSet<&'static str> = BTreeSet::new();
        for (path, score) in self.0.get(family).map_or(&[][..], Vec::as_slice) {
            if *score >= MIN_SCORE
                && let Some(t) = map_path(path)
            {
                set.insert(t);
            }
        }
        if categories.contains(&"MONOSPACE") {
            set.insert("Monospace");
        }
        if set.is_empty() {
            set.extend(categories.iter().find_map(|c| from_category(c)));
        }
        set.into_iter().map(str::to_string).collect()
    }
}

fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.trim_end_matches('\r').chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Hand-written tags that replace the computed ones, from `curation.toml`.
#[derive(Default)]
pub struct Curation(BTreeMap<String, Vec<String>>);

impl Curation {
    /// Reads `curation.toml` from the working directory (the repository root when run through
    /// `scripts/regenerate.sh`); absent means no overrides.
    pub fn load_default() -> Result<Curation, String> {
        match std::fs::read_to_string("curation.toml") {
            Ok(t) => Curation::parse(&t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Curation::default()),
            Err(e) => Err(format!("curation.toml: {e}")),
        }
    }

    /// Lines of the form `"Family name" = ["Group/Trait", ...]`; `#` starts a comment line.
    pub fn parse(text: &str) -> Result<Curation, String> {
        let mut m = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let err = |what: &str| format!("curation.toml:{}: {what}", n + 1);
            let (key, val) = line
                .split_once("\" =")
                .ok_or_else(|| err("expected `\"Family\" = [..]`"))?;
            let key = key
                .strip_prefix('"')
                .ok_or_else(|| err("the family name needs quotes"))?;
            let mut tags: Vec<String> =
                serde_json::from_str(val.trim()).map_err(|e| err(&format!("bad tag list: {e}")))?;
            if let Some(bad) = tags.iter().find(|t| !VOCABULARY.contains(&t.as_str())) {
                return Err(err(&format!("unknown tag `{bad}`")));
            }
            tags.sort();
            tags.dedup();
            m.insert(key.to_string(), tags);
        }
        Ok(Curation(m))
    }

    pub fn get(&self, family: &str) -> Option<&[String]> {
        self.0.get(family).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "\
Abc,,/Sans/Geometric,80
Abc,,/Sans/Humanist,49.9
Abc,,/Expressive/Calm,50
Abc,,/Theme/Unmapped,99
Abc,wght,/Theme/Pixel,90
\"Comma, Family\",,/Slab/Clarendon,60
Fallback,,/Sans/Humanist,10
";

    #[test]
    fn threshold_and_axes_rows() {
        let t = GoogleTags::parse(CSV);
        assert_eq!(
            t.tags_for("Abc", &["SANS_SERIF"]),
            ["Feeling/Calm", "Sans/Geometric"]
        );
        assert_eq!(t.tags_for("Comma, Family", &["SERIF"]), ["Serif/Slab"]);
    }

    #[test]
    fn fallback_from_category_when_nothing_maps() {
        let t = GoogleTags::parse(CSV);
        assert_eq!(
            t.tags_for("Fallback", &["HANDWRITING"]),
            ["Script/Handwritten"]
        );
        assert_eq!(t.tags_for("Unknown", &["DISPLAY"]), ["Display"]);
        assert!(!t.has_mapped("Fallback"));
        assert!(t.has_mapped("Abc"));
    }

    #[test]
    fn monospace_category_adds_tag() {
        let t = GoogleTags::parse(CSV);
        assert_eq!(
            t.tags_for("Abc", &["MONOSPACE"]),
            ["Feeling/Calm", "Monospace", "Sans/Geometric"]
        );
    }

    #[test]
    fn mapped_tags_are_in_the_vocabulary() {
        for p in [
            "/Sans/Neo Grotesque",
            "/Serif/Fat Face",
            "/Slab/Geometric",
            "/Theme/Art Nouveau",
            "/Expressive/Fancy",
            "/Monospace/Monospace",
            "/Theme/Brush",
        ] {
            assert!(VOCABULARY.contains(&map_path(p).unwrap()), "{p}");
        }
    }

    #[test]
    fn curation_parses_and_rejects_unknown_tags() {
        let c = Curation::parse("# c\n\"Foo\" = [\"Display/Playful\", \"Sans\"]\n").unwrap();
        assert_eq!(c.get("Foo").unwrap(), ["Display/Playful", "Sans"]);
        assert!(Curation::parse("\"Foo\" = [\"Nope\"]").is_err());
    }
}
