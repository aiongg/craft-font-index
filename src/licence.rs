//! Copyright lines and Reserved Font Names from the top of an OFL.txt.

const MAX_HEADER_LINES: usize = 20;

/// The copyright notice at the top of a licence file: its lines up to the licence text
/// ("This Font Software is licensed under ..."), joined by single spaces. None when empty.
pub fn copyright_header(text: &str) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for line in text
        .trim_start_matches('\u{feff}')
        .lines()
        .take(MAX_HEADER_LINES)
    {
        let l = line.trim();
        let lower = l.to_lowercase();
        if lower.contains("is licensed under")
            || lower.starts_with("sil open font license")
            || l.starts_with("----")
        {
            break;
        }
        if !l.is_empty() {
            lines.push(l);
        }
    }
    let joined = lines.join(" ");
    let joined = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// The Reserved Font Names a copyright notice declares: `with Reserved Font Name 'X'`,
/// `Reserved Font Name X.` or `Reserved Font Names "A" and "B".`. Empty when it has none.
pub fn reserved_names(copyright: &str) -> Vec<String> {
    const KEY: &str = "reserved font name";
    // ASCII lowercasing keeps byte offsets equal to the original's.
    let lower = copyright.to_ascii_lowercase();
    let Some(at) = lower.find(KEY) else {
        return Vec::new();
    };
    let mut rest = &copyright[at + KEY.len()..];
    rest = rest.strip_prefix(['s', 'S']).unwrap_or(rest).trim_start();
    let mut names: Vec<String> = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let close = match chars[i] {
            '"' => '"',
            '\'' => '\'',
            '\u{201c}' => '\u{201d}',
            '\u{2018}' => '\u{2019}',
            _ => {
                i += 1;
                continue;
            }
        };
        match chars[i + 1..].iter().position(|&c| c == close) {
            Some(n) => {
                let name: String = chars[i + 1..i + 1 + n].iter().collect();
                let name = name.trim();
                if !name.is_empty() && !names.iter().any(|x| x == name) {
                    names.push(name.to_string());
                }
                i += n + 2;
            }
            None => break,
        }
    }
    if names.is_empty() {
        // Unquoted: the rest of the sentence.
        let name = rest.trim().trim_end_matches('.').trim();
        let name = name.trim_matches(['"', '\'']);
        if !name.is_empty() {
            names.push(name.to_string());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rfn(s: &str) -> Vec<String> {
        reserved_names(&copyright_header(s).unwrap_or_default())
    }

    #[test]
    fn single_quoted_name() {
        let c = "Copyright 2011 The ABeeZee Project Authors (https://github.com/googlefonts/abeezee) with Reserved Font Name 'ABeeZee'\n\nThis Font Software is licensed under the SIL Open Font License";
        assert_eq!(rfn(c), ["ABeeZee"]);
    }

    #[test]
    fn unquoted_name_with_full_stop() {
        assert_eq!(
            rfn(
                "Copyright (c) 2010, Someone (a@b.c),\nwith Reserved Font Name Abel.\n\nThis Font Software is licensed under the SIL Open Font License"
            ),
            ["Abel"]
        );
    }

    #[test]
    fn several_quoted_names_across_lines() {
        let c = "Copyright (c) 2011, TypeTogether (www.type-together.com), \nwith Reserved Font Names \"Abril\" and \"Abril Fatface\"\n\n\nThis Font Software is licensed under";
        assert_eq!(rfn(c), ["Abril", "Abril Fatface"]);
    }

    #[test]
    fn quoted_names_followed_by_full_stop() {
        let c = "This Font Software is Copyright (c) 2000-2024, SIL Global (https://www.sil.org/)\nwith Reserved Font Names \"Abyssinica\" and \"SIL\".\n\nThis Font Software is licensed under";
        assert_eq!(rfn(c), ["Abyssinica", "SIL"]);
    }

    #[test]
    fn no_reserved_names_and_licence_text_is_ignored() {
        let c = "Copyright 2022 The Noto Project Authors (https://github.com/notofonts/adlam)\n\nThis Font Software is licensed under the SIL Open Font License, Version 1.1.\nwith Reserved Font Name 'Nope'";
        assert!(rfn(c).is_empty());
        assert_eq!(
            copyright_header(c).as_deref(),
            Some("Copyright 2022 The Noto Project Authors (https://github.com/notofonts/adlam)")
        );
    }
}
