//! Default preview samples: one text per script ([`choose`]) and which one each font file
//! shows ([`pick`]).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use unicode_script::UnicodeScript;

use crate::lang::Language;

/// Samples and a file's own text are cut to this many characters.
pub const CAP: usize = 60;
/// A file shows a sample when it maps at least this share of the sample's characters.
const MIN_COVERAGE: f64 = 0.8;
/// A file without primary script is Latin when it maps this share of the Latin sample's letters.
const LATIN_COVERAGE: f64 = 0.9;
/// A file shows its own characters of a script when it has at least this many.
const MIN_OWN: usize = 10;

/// Population ranks Wu above Mandarin for Traditional Chinese; prefer the standard language.
const PREFER: &[(&str, &str)] = &[("Hant", "zh_Hant")];
/// Script codes that fonts use but gflanguages languages don't.
const ALIAS: &[(&str, &str)] = &[
    ("Hira", "Jpan"),
    ("Kana", "Jpan"),
    ("Hang", "Kore"),
    ("Hani", "Hans"),
];
/// Kana-only fonts can't show the Japanese UDHR sentence, which has kanji. The Iroha uses each
/// kana once.
const IROHA: &str = "いろはにほへと ちりぬるを わかよたれそ つねならむ うゐのおくやま けふこえて あさきゆめみし ゑひもせす";

pub struct Sample {
    pub text: String,
    /// `udhr`, `iroha` or `exemplar`.
    pub method: &'static str,
    /// The gflanguages language ID the text comes from.
    pub language: String,
}

impl Sample {
    pub fn to_json(&self) -> Value {
        json!({"text": self.text, "method": self.method, "language": self.language})
    }
}

fn cut(s: &str) -> String {
    s.chars().take(CAP).collect()
}

/// The sample of every script the languages use, plus the aliases in [`ALIAS`]: the UDHR text
/// of the language with the largest population (zh_Hant for Hant), else that language's
/// exemplar characters. Hira gets the Iroha. Scripts with neither are left out.
pub fn choose(languages: &[Language]) -> BTreeMap<String, Sample> {
    let mut scripts: BTreeSet<&str> = languages.iter().map(|l| l.script.as_str()).collect();
    scripts.extend(ALIAS.iter().map(|(s, _)| *s));
    let mut out = BTreeMap::new();
    for s in scripts {
        let key = ALIAS.iter().find(|(a, _)| *a == s).map_or(s, |(_, k)| *k);
        let prefer = PREFER.iter().find(|(p, _)| *p == key).map(|(_, l)| *l);
        let cands = || languages.iter().filter(|l| l.script == key);
        let udhr = cands().filter(|l| l.sample_text.is_some()).min_by_key(|l| {
            (
                Some(l.id.as_str()) != prefer,
                std::cmp::Reverse(l.population),
            )
        });
        let sample = if let Some(l) = udhr {
            Sample {
                text: cut(l.sample_text.as_deref().unwrap_or_default()),
                method: "udhr",
                language: l.id.clone(),
            }
        } else if let Some(l) = cands()
            .filter(|l| l.exemplar.is_some())
            .min_by_key(|l| std::cmp::Reverse(l.population))
        {
            let ex = l
                .exemplar
                .as_deref()
                .unwrap_or_default()
                .replace(['{', '}'], "");
            Sample {
                text: cut(&ex.split_whitespace().collect::<String>()),
                method: "exemplar",
                language: l.id.clone(),
            }
        } else {
            continue;
        };
        out.insert(s.to_string(), sample);
    }
    out.insert(
        "Hira".into(),
        Sample {
            text: cut(IROHA),
            method: "iroha",
            language: "ja_Jpan".into(),
        },
    );
    out
}

/// The ISO 15924 code of a code point's Script property (`Zyyy`, `Zinh`, `Zzzz` included).
fn script_of(cp: u32) -> &'static str {
    char::from_u32(cp).map_or("Zzzz", |c| c.script().short_name())
}

/// The share of `text`'s non-space characters that `cmap` maps.
fn coverage(cmap: &BTreeSet<u32>, text: &str) -> f64 {
    let want: BTreeSet<u32> = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(u32::from)
        .collect();
    if want.is_empty() {
        return 0.0;
    }
    want.iter().filter(|c| cmap.contains(c)).count() as f64 / want.len() as f64
}

/// The script of a file without primary script: Latin if it maps the Latin sample's letters,
/// else the script with the most mapped code points, else `Zsym` (emoji, icons).
fn detect(cmap: &BTreeSet<u32>, latin: &str) -> String {
    let letters: BTreeSet<u32> = latin
        .chars()
        .filter(|c| c.is_alphabetic())
        .map(u32::from)
        .collect();
    if !letters.is_empty()
        && letters.iter().filter(|c| cmap.contains(c)).count() as f64 / letters.len() as f64
            >= LATIN_COVERAGE
    {
        return "Latn".into();
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for &cp in cmap {
        let s = script_of(cp);
        if matches!(s, "Zyyy" | "Zinh" | "Zzzz" | "Latn") {
            continue;
        }
        match counts.iter_mut().find(|(k, _)| *k == s) {
            Some((_, n)) => *n += 1,
            None => counts.push((s, 1)),
        }
    }
    // On a tie, the script whose first code point is lowest.
    let mut best: Option<(&str, usize)> = None;
    for (s, n) in counts {
        if best.is_none_or(|(_, b)| n > b) {
            best = Some((s, n));
        }
    }
    best.map_or_else(|| "Zsym".into(), |(s, _)| s.to_string())
}

/// The file's first [`CAP`] mapped characters of `script` above U+0020; for `Zsym`, of any
/// script.
fn own_text(cmap: &BTreeSet<u32>, script: &str) -> String {
    cmap.iter()
        .filter(|&&cp| cp > 0x20 && (script == "Zsym" || script_of(cp) == script))
        .filter_map(|&cp| char::from_u32(cp))
        .take(CAP)
        .collect()
}

/// What a font file shows: the sample of `script`, or with `text`, those characters.
#[derive(Debug, PartialEq)]
pub struct Pick {
    pub script: String,
    pub text: Option<String>,
}

/// The sample a file with code points `cmap` shows. Its script is `primary`, else detected from
/// the cmap. When the file maps under 80% of that script's sample: its own characters of the
/// script if it has 10; else the sample of the script detected from the cmap if it covers that;
/// else its own characters of either script; else its first characters of any script.
pub fn pick(
    samples: &BTreeMap<String, Sample>,
    primary: Option<&str>,
    cmap: &BTreeSet<u32>,
) -> Pick {
    let latin = samples.get("Latn").map_or("", |s| s.text.as_str());
    let covers = |s: &str| {
        samples
            .get(s)
            .is_some_and(|x| coverage(cmap, &x.text) >= MIN_COVERAGE)
    };
    let script = primary
        .filter(|p| !p.is_empty())
        .map_or_else(|| detect(cmap, latin), str::to_string);
    if covers(&script) {
        return Pick { script, text: None };
    }
    let own = own_text(cmap, &script);
    if own.chars().count() >= MIN_OWN {
        return Pick {
            script,
            text: Some(own),
        };
    }
    let alt = detect(cmap, latin);
    if covers(&alt) {
        return Pick {
            script: alt,
            text: None,
        };
    }
    for s in [script, alt.clone()] {
        let own = own_text(cmap, &s);
        if !own.is_empty() {
            return Pick {
                script: s,
                text: Some(own),
            };
        }
    }
    Pick {
        script: alt,
        text: Some(own_text(cmap, "Zsym")),
    }
}

/// Writes `pick` into a file record as `sample_script` and, when the file shows its own
/// characters, `sample_text`. [`settle`] later drops the `sample_script`s the family shares.
pub fn apply(file: &mut Value, pick: Pick) {
    file["sample_script"] = json!(pick.script);
    if let Some(t) = pick.text.filter(|t| !t.is_empty()) {
        file["sample_text"] = json!(t);
    }
}

/// The family's `sample_script`: the script most of its files show (on a tie `primary`, else
/// the first by code). Removes it from the files that show it, so a file carries
/// `sample_script` only when it differs.
pub fn settle(files: &mut [Value], primary: Option<&str>) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in files.iter() {
        if let Some(s) = f["sample_script"].as_str() {
            *counts.entry(s.to_string()).or_default() += 1;
        }
    }
    let max = *counts.values().max()?;
    let tied: Vec<&String> = counts
        .iter()
        .filter(|(_, n)| **n == max)
        .map(|(s, _)| s)
        .collect();
    let best = tied
        .iter()
        .find(|s| Some(s.as_str()) == primary)
        .unwrap_or(&tied[0])
        .to_string();
    for f in files.iter_mut() {
        if f["sample_script"].as_str() == Some(&best)
            && let Some(o) = f.as_object_mut()
        {
            o.remove("sample_script");
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lang(id: &str, population: u64, sample: Option<&str>, exemplar: Option<&str>) -> Language {
        Language {
            id: id.into(),
            name: id.into(),
            autonym: None,
            script: id.split('_').nth(1).unwrap_or_default().into(),
            base: BTreeSet::new(),
            exemplar: exemplar.map(str::to_string),
            sample_text: sample.map(str::to_string),
            historical: false,
            population,
        }
    }

    fn cps(s: &str) -> BTreeSet<u32> {
        s.chars().map(u32::from).collect()
    }

    fn languages() -> Vec<Language> {
        vec![
            lang(
                "en_Latn",
                1_000,
                Some("All human beings are born free and equal"),
                None,
            ),
            lang("gu_Gujr", 50, Some("પ્રસ્તાવના"), None),
            lang("ja_Jpan", 100, Some("人類社会のすべての構成員"), None),
            lang("wuu_Hant", 80, Some("吳語樣本"), None),
            lang("zh_Hant", 30, Some("鑑於對人類家庭"), None),
            lang("bax_Bamu", 1, None, Some("ꚠ ꚡ {ꚢ}")),
        ]
    }

    #[test]
    fn script_samples() {
        let s = choose(&languages());
        assert_eq!(s["Hant"].language, "zh_Hant");
        assert_eq!(s["Hira"].method, "iroha");
        assert_eq!(s["Kana"].language, "ja_Jpan");
        assert_eq!(s["Bamu"].text, "ꚠꚡꚢ");
        assert_eq!(s["Bamu"].method, "exemplar");
        assert!(s["Latn"].text.chars().count() <= CAP);
    }

    #[test]
    fn kana_only_font_gets_iroha() {
        let s = choose(&languages());
        let cmap = cps(&format!("0123456789{IROHA}"));
        let p = pick(&s, None, &cmap);
        assert_eq!(
            p,
            Pick {
                script: "Hira".into(),
                text: None
            }
        );
    }

    #[test]
    fn fallbacks() {
        let s = choose(&languages());
        let latin = cps("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        // A Gujarati family's italic without Gujarati shows the Latin sample.
        let p = pick(&s, Some("Gujr"), &latin);
        assert_eq!(
            p,
            Pick {
                script: "Latn".into(),
                text: None
            }
        );
        // A script without sample text: the file's own characters.
        let linear_a: BTreeSet<u32> = (0x10600..0x10650).collect();
        let p = pick(&s, Some("Lina"), &linear_a);
        assert_eq!(p.script, "Lina");
        assert_eq!(p.text.unwrap().chars().count(), CAP);
        // Symbols only: the first characters of any script.
        let emoji: BTreeSet<u32> = [0x23, 0x2A, 0x1F600, 0x1F601].into();
        let p = pick(&s, None, &emoji);
        assert_eq!(
            p,
            Pick {
                script: "Zsym".into(),
                text: Some("#*😀😁".into())
            }
        );
    }

    #[test]
    fn family_takes_the_most_common_script() {
        let mut files = vec![
            json!({"path": "a", "sample_script": "Gujr"}),
            json!({"path": "b", "sample_script": "Latn"}),
        ];
        assert_eq!(settle(&mut files, Some("Gujr")).as_deref(), Some("Gujr"));
        assert!(files[0].get("sample_script").is_none());
        assert_eq!(files[1]["sample_script"], "Latn");
    }
}
