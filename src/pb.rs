//! Protobuf text format, as used by google/fonts' METADATA.pb and the gflanguages textproto
//! files. A small parser rather than a protobuf dependency, because the generator only needs
//! a handful of fields and must not fail on fields added later. Written by @kntrttm for
//! photocraft#2453.

const MAX_DEPTH: usize = 32;

/// A parsed value: scalar (string, number or enum identifier, kept as text) or nested message.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Scalar(String),
    Msg(Msg),
}

impl Val {
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            Val::Scalar(s) => Some(s),
            Val::Msg(_) => None,
        }
    }
    pub fn as_msg(&self) -> Option<&Msg> {
        match self {
            Val::Msg(m) => Some(m),
            Val::Scalar(_) => None,
        }
    }
}

/// Ordered `(field, value)` pairs; fields may repeat.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Msg(pub Vec<(String, Val)>);

impl Msg {
    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Val> {
        self.0.iter().filter(move |(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn str<'a>(&'a self, key: &str) -> Option<&'a str> {
        self.0
            .iter()
            .filter(|(k, _)| k == key)
            .find_map(|(_, v)| v.as_scalar())
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        self.str(key)?.parse().ok().filter(|n: &f64| n.is_finite())
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Colon,
    Open,
    Close,
    LBracket,
    RBracket,
    Comma,
    Semi,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while let Some(&c) = b.get(i) {
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'#' => {
                while b.get(i).is_some_and(|&c| c != b'\n') {
                    i += 1;
                }
            }
            b':' => {
                out.push(Tok::Colon);
                i += 1;
            }
            b'{' | b'<' => {
                out.push(Tok::Open);
                i += 1;
            }
            b'}' | b'>' => {
                out.push(Tok::Close);
                i += 1;
            }
            b'[' => {
                out.push(Tok::LBracket);
                i += 1;
            }
            b']' => {
                out.push(Tok::RBracket);
                i += 1;
            }
            b',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            b';' => {
                out.push(Tok::Semi);
                i += 1;
            }
            b'"' | b'\'' => {
                let quote = c;
                i += 1;
                let mut bytes = Vec::new();
                loop {
                    let Some(&c) = b.get(i) else {
                        return Err("unterminated string".into());
                    };
                    i += 1;
                    if c == quote {
                        break;
                    }
                    if c == b'\n' {
                        return Err("newline in string".into());
                    }
                    if c != b'\\' {
                        bytes.push(c);
                        continue;
                    }
                    let Some(&e) = b.get(i) else {
                        return Err("unterminated escape".into());
                    };
                    i += 1;
                    match e {
                        b'n' => bytes.push(b'\n'),
                        b't' => bytes.push(b'\t'),
                        b'r' => bytes.push(b'\r'),
                        b'x' | b'X' => {
                            let mut v = 0u32;
                            let mut n = 0;
                            while n < 2 && b.get(i).is_some_and(u8::is_ascii_hexdigit) {
                                v = v * 16
                                    + (*b.get(i).unwrap_or(&b'0') as char)
                                        .to_digit(16)
                                        .unwrap_or(0);
                                i += 1;
                                n += 1;
                            }
                            bytes.push(v as u8);
                        }
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            let mut n = 1;
                            while n < 3 && b.get(i).is_some_and(|d| (b'0'..=b'7').contains(d)) {
                                v = v * 8 + u32::from(b.get(i).copied().unwrap_or(b'0') - b'0');
                                i += 1;
                                n += 1;
                            }
                            bytes.push(v as u8);
                        }
                        other => bytes.push(other),
                    }
                }
                out.push(Tok::Str(String::from_utf8_lossy(&bytes).into_owned()));
            }
            _ => {
                let start = i;
                while b.get(i).is_some_and(|&c| {
                    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'+')
                }) {
                    i += 1;
                }
                if i == start {
                    return Err(format!("unexpected byte 0x{c:02x}"));
                }
                out.push(Tok::Ident(src.get(start..i).unwrap_or("").to_string()));
            }
        }
    }
    Ok(out)
}

/// Parses protobuf text format into a [`Msg`]. Unknown fields are kept (callers ignore them).
pub fn parse_pb(src: &str) -> Result<Msg, String> {
    let toks = lex(src)?;
    let mut pos = 0;
    let m = parse_msg(&toks, &mut pos, 0, true)?;
    Ok(m)
}

fn parse_msg(t: &[Tok], pos: &mut usize, depth: usize, top: bool) -> Result<Msg, String> {
    if depth > MAX_DEPTH {
        return Err("nesting too deep".into());
    }
    let mut fields = Vec::new();
    loop {
        match t.get(*pos) {
            None if top => return Ok(Msg(fields)),
            None => return Err("unclosed `{`".into()),
            Some(Tok::Close) if !top => {
                *pos += 1;
                return Ok(Msg(fields));
            }
            Some(Tok::Close) => return Err("unexpected `}`".into()),
            Some(Tok::Semi | Tok::Comma) => *pos += 1,
            Some(Tok::Ident(key)) => {
                *pos += 1;
                let had_colon = matches!(t.get(*pos), Some(Tok::Colon));
                if had_colon {
                    *pos += 1;
                }
                match t.get(*pos) {
                    Some(Tok::Open) => {
                        *pos += 1;
                        let m = parse_msg(t, pos, depth + 1, false)?;
                        fields.push((key.clone(), Val::Msg(m)));
                    }
                    Some(Tok::LBracket) if had_colon => {
                        *pos += 1;
                        loop {
                            match t.get(*pos) {
                                Some(Tok::RBracket) => {
                                    *pos += 1;
                                    break;
                                }
                                Some(Tok::Comma) => *pos += 1,
                                Some(Tok::Open) => {
                                    *pos += 1;
                                    let m = parse_msg(t, pos, depth + 1, false)?;
                                    fields.push((key.clone(), Val::Msg(m)));
                                }
                                Some(_) => {
                                    let s = scalar(t, pos)?;
                                    fields.push((key.clone(), Val::Scalar(s)));
                                }
                                None => return Err("unclosed `[`".into()),
                            }
                        }
                    }
                    Some(Tok::Ident(_) | Tok::Str(_)) if had_colon => {
                        let s = scalar(t, pos)?;
                        fields.push((key.clone(), Val::Scalar(s)));
                    }
                    _ => return Err(format!("field `{key}` has no value")),
                }
            }
            Some(other) => return Err(format!("unexpected token {other:?}")),
        }
    }
}

/// A scalar: an identifier/number, or one or more adjacent string literals (concatenated).
fn scalar(t: &[Tok], pos: &mut usize) -> Result<String, String> {
    match t.get(*pos) {
        Some(Tok::Ident(s)) => {
            *pos += 1;
            Ok(s.clone())
        }
        Some(Tok::Str(_)) => {
            let mut s = String::new();
            while let Some(Tok::Str(p)) = t.get(*pos) {
                s.push_str(p);
                *pos += 1;
            }
            Ok(s)
        }
        _ => Err("expected a value".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# a comment
name: "Noto Sans JP"
designer: "Say \"hi\"\n\x41\101 # not a comment"
license: "OFL"
category: "SANS_SERIF"
category: "DISPLAY"
date_added: "2014-12-31"
fonts {
  name: "Noto Sans JP"
  style: "normal"
  weight: 400
  filename: "NotoSansJP[wght].ttf"
  post_script_name: "NotoSansJP-Regular"
  full_name: "Noto Sans JP Regular"
  copyright: "Copyright 2014" " Adobe"
}
subsets: "japanese"
subsets: "latin"
axes {
  tag: "wght"
  min_value: 100.0
  max_value: 900.0
}
source {
  repository_url: "https://github.com/x/y"
  files { source_file: "a" dest_file: "b" }
  branch: "main"
}
languages: ["en", "ja"]
stroke: SANS_SERIF
primary_script: ""
"#;

    #[test]
    fn parses_sample() {
        let m = parse_pb(SAMPLE).unwrap();
        assert_eq!(m.str("name"), Some("Noto Sans JP"));
        assert_eq!(m.str("designer"), Some("Say \"hi\"\nAA # not a comment"));
        assert_eq!(
            m.all("category")
                .filter_map(Val::as_scalar)
                .collect::<Vec<_>>(),
            ["SANS_SERIF", "DISPLAY"]
        );
        assert_eq!(m.all("subsets").count(), 2);
        let f = m.all("fonts").find_map(Val::as_msg).unwrap();
        assert_eq!(f.num("weight"), Some(400.0));
        assert_eq!(f.str("copyright"), Some("Copyright 2014 Adobe"));
        let a = m.all("axes").find_map(Val::as_msg).unwrap();
        assert_eq!(
            (a.num("min_value"), a.num("max_value")),
            (Some(100.0), Some(900.0))
        );
        assert_eq!(m.all("languages").count(), 2);
        assert_eq!(m.str("stroke"), Some("SANS_SERIF"));
    }

    #[test]
    fn malformed_is_error_not_panic() {
        for s in [
            "name: \"x",
            "fonts { name: \"x\"",
            "}",
            "name",
            "name: ",
            "name: {",
            "fonts { ] }",
            "\u{1}",
            "a: \"\\",
            "x: [1,",
            "name: \"a\nb\"",
        ] {
            assert!(parse_pb(s).is_err(), "{s:?}");
        }
        let deep = "a {".repeat(200) + &"}".repeat(200);
        assert!(parse_pb(&deep).is_err());
        let _ = parse_pb("\u{feff}name: \"é\"");
        assert!(parse_pb("").unwrap().0.is_empty());
    }
}
