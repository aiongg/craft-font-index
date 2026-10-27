//! The gflanguages data in google/fonts (`lang/Lib/gflanguages/data`): languages with their
//! script, base exemplar characters and UDHR sample text, and the script list.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::pb::{Val, parse_pb};
use crate::sample::{self, Sample};

pub const DATA_DIR: &str = "lang/Lib/gflanguages/data";

pub struct Language {
    /// `<language>_<Script>`, for example `vi_Latn`.
    pub id: String,
    pub name: String,
    pub autonym: Option<String>,
    pub script: String,
    /// Every code point of the base exemplar characters. Empty when the data has none, in
    /// which case coverage is never inferred for the language.
    pub base: BTreeSet<u32>,
    /// The base exemplar characters as written in the data.
    pub exemplar: Option<String>,
    /// UDHR text from `sample_text`: `styles` (the sentence fonts.google.com shows), else the
    /// first of the longer fields in [`SAMPLE_FIELDS`].
    pub sample_text: Option<String>,
    pub historical: bool,
    pub population: u64,
}

pub struct LangData {
    pub languages: Vec<Language>,
    /// ISO 15924 code by lowercased, hyphenated script name (`old-italic` -> `Ital`).
    pub script_by_name: BTreeMap<String, String>,
    /// Script name by ISO 15924 code.
    pub scripts: BTreeMap<String, String>,
    /// The default preview sample by ISO 15924 code ([`sample::choose`]).
    pub samples: BTreeMap<String, Sample>,
}

/// `sample_text` fields, best first.
pub const SAMPLE_FIELDS: &[&str] = &[
    "styles",
    "tester",
    "specimen_48",
    "specimen_36",
    "specimen_32",
    "specimen_21",
    "specimen_16",
    "poster_sm",
    "poster_md",
];

/// google/fonts subset names that are not a script name in the lang data.
const SUBSET_SCRIPTS: &[(&str, &str)] = &[
    ("latin-ext", "Latn"),
    ("vietnamese", "Latn"),
    ("cyrillic-ext", "Cyrl"),
    ("greek-ext", "Grek"),
    ("japanese", "Jpan"),
    ("kana-extended", "Kana"),
    ("korean", "Kore"),
    ("chinese-simplified", "Hans"),
    ("chinese-traditional", "Hant"),
    ("chinese-hongkong", "Hant"),
    ("tamil-supplement", "Taml"),
    ("braille", "Brai"),
    ("mayan-numerals", "Maya"),
    ("math", "Zmth"),
    ("symbols", "Zsym"),
    ("symbols2", "Zsym"),
    ("music", "Zsym"),
    ("znamenny", "Zsym"),
    ("emoji", "Zsye"),
    ("ottoman-siyaq-numbers", "Arab"),
    ("indic-siyaq-numbers", "Arab"),
    ("bengali", "Beng"),
    ("canadian-aboriginal", "Cans"),
    ("cuneiform", "Xsux"),
    ("cypro-minoan", "Cpmn"),
    ("duployan", "Dupl"),
    ("khitan-small-script", "Kits"),
    ("lisu", "Lisu"),
    ("mandaic", "Mand"),
    ("mende-kikakui", "Mend"),
    ("meroitic", "Mero"),
    ("miao", "Plrd"),
    ("nushu", "Nshu"),
    ("old-turkic", "Orkh"),
    ("oriya", "Orya"),
    ("signwriting", "Sgnw"),
    ("tai-tham", "Lana"),
    ("warang-citi", "Wara"),
    ("zanabazar-square", "Zanb"),
];

/// Names of the codes in `SUBSET_SCRIPTS` that the lang data has no file for.
const EXTRA_SCRIPT_NAMES: &[(&str, &str)] = &[
    ("Cpmn", "Cypro-Minoan"),
    ("Kits", "Khitan Small Script"),
    ("Maya", "Mayan Numerals"),
    ("Sgnw", "SignWriting"),
    ("Zmth", "Mathematical Notation"),
    ("Zsye", "Emoji"),
    ("Zsym", "Symbols"),
];

fn slug(name: &str) -> String {
    name.replace(['’', '\''], "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

/// The code points of a CLDR exemplar string: space-separated items, where `{xy}` is a
/// cluster and `a-z` a range.
pub fn exemplar_codepoints(s: &str) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for item in s.split_whitespace() {
        let item = item.trim_start_matches('{').trim_end_matches('}');
        let chars: Vec<char> = item.chars().collect();
        if let [a, '-', b] = chars[..]
            && a < b
            && (b as u32) - (a as u32) < 0x10000
        {
            out.extend(a as u32..=b as u32);
            continue;
        }
        // The dotted circle only carries a combining mark in the data.
        out.extend(chars.iter().map(|&c| c as u32).filter(|&c| c != 0x25CC));
    }
    out
}

fn read_dir_sorted(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "textproto"))
        .collect();
    v.sort();
    Ok(v)
}

impl LangData {
    pub fn load(repo: &Path, warnings: &mut Vec<String>) -> Result<LangData, String> {
        let dir = repo.join(DATA_DIR);
        let mut script_by_name = BTreeMap::new();
        let mut scripts: BTreeMap<String, String> = EXTRA_SCRIPT_NAMES
            .iter()
            .map(|(c, n)| (c.to_string(), n.to_string()))
            .collect();
        for p in read_dir_sorted(&dir.join("scripts"))? {
            let text =
                std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
            let Ok(m) = parse_pb(&text) else {
                warnings.push(format!("{}: malformed", p.display()));
                continue;
            };
            if let (Some(id), Some(name)) = (m.str("id"), m.str("name")) {
                script_by_name.insert(slug(name), id.to_string());
                scripts.insert(id.to_string(), name.to_string());
            }
        }
        let mut languages = Vec::new();
        for p in read_dir_sorted(&dir.join("languages"))? {
            let text =
                std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
            let Ok(m) = parse_pb(&text) else {
                warnings.push(format!("{}: malformed", p.display()));
                continue;
            };
            let (Some(id), Some(script), Some(name)) =
                (m.str("id"), m.str("script"), m.str("name"))
            else {
                continue;
            };
            let exemplar = m
                .all("exemplar_chars")
                .filter_map(Val::as_msg)
                .find_map(|e| e.str("base"))
                .filter(|b| !b.is_empty());
            let base = exemplar.map(exemplar_codepoints).unwrap_or_default();
            let sample_text = m
                .all("sample_text")
                .filter_map(Val::as_msg)
                .next()
                .and_then(|t| {
                    SAMPLE_FIELDS
                        .iter()
                        .find_map(|f| t.str(f).filter(|s| !s.is_empty()))
                });
            languages.push(Language {
                id: id.to_string(),
                name: m.str("preferred_name").unwrap_or(name).to_string(),
                autonym: m
                    .str("autonym")
                    .filter(|a| !a.is_empty())
                    .map(str::to_string),
                script: script.to_string(),
                base,
                exemplar: exemplar.map(str::to_string),
                sample_text: sample_text.map(str::to_string),
                historical: m.str("historical") == Some("true"),
                population: m.num("population").map_or(0, |n| n.max(0.0) as u64),
            });
        }
        let samples = sample::choose(&languages);
        Ok(LangData {
            languages,
            script_by_name,
            scripts,
            samples,
        })
    }

    /// The script of a language ID (`bsc_Latn_GN` -> `Latn`), also for IDs not in the data.
    pub fn script_of(&self, id: &str) -> Option<String> {
        match self.languages.iter().find(|l| l.id == id) {
            Some(l) => Some(l.script.clone()),
            None => id.split('_').nth(1).map(str::to_string),
        }
    }

    /// The ISO 15924 code of a google/fonts subset, or `None` for `menu` and unknown names.
    pub fn subset_script(&self, subset: &str) -> Option<&str> {
        SUBSET_SCRIPTS
            .iter()
            .find(|(s, _)| *s == subset)
            .map(|(_, c)| *c)
            .or_else(|| self.script_by_name.get(subset).map(String::as_str))
    }

    /// Languages a font with these code points supports: every base exemplar character is
    /// mapped. Historical languages and languages without a known speaker population are
    /// left out, so the list is about writing living languages.
    pub fn covered(&self, cmap: &BTreeSet<u32>) -> Vec<&Language> {
        self.languages
            .iter()
            .filter(|l| !l.historical && l.population > 0 && !l.base.is_empty())
            .filter(|l| l.base.is_subset(cmap))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemplars() {
        let cps = exemplar_codepoints("a b {ch} {a̱} 丘-业 ◌́");
        let want: BTreeSet<u32> = ['a', 'b', 'c', 'h', '\u{331}', '\u{301}']
            .iter()
            .map(|&c| c as u32)
            .chain(0x4E18..=0x4E1A)
            .collect();
        assert_eq!(cps, want);
        assert_eq!(
            exemplar_codepoints("- , ."),
            BTreeSet::from(['-' as u32, ',' as u32, '.' as u32])
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Old Italic"), "old-italic");
        assert_eq!(slug("Nyiakeng Puachue Hmong"), "nyiakeng-puachue-hmong");
        assert_eq!(slug("N’Ko"), "nko");
    }
}
