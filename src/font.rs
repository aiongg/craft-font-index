//! What the index reads from a font file itself.

use std::collections::BTreeSet;

use skrifa::{FontRef, MetadataProvider, string::StringId};

#[derive(Default)]
pub struct FontInfo {
    /// `None` when the font could not be parsed (axes unknown), `Some(vec![])` for static fonts.
    pub axes: Option<Vec<(String, f32, f32)>>,
    pub postscript: Vec<String>,
    /// Mapped code points, for language coverage.
    pub codepoints: BTreeSet<u32>,
}

/// Axes, PostScript names and mapped code points, read with skrifa. The PostScript names are
/// name ID 6 and, for each `fvar` named instance, its own PostScript name, or (name ID 25 prefix
/// + `-` + subfamily without spaces) when it has none.
pub fn read_font(data: &[u8]) -> FontInfo {
    let Ok(font) = FontRef::new(data) else {
        return FontInfo::default();
    };
    let first = |id: StringId| {
        font.localized_strings(id)
            .next()
            .map(|s| s.chars().collect::<String>())
            .filter(|s| !s.is_empty())
    };
    let mut ps = Vec::new();
    ps.extend(first(StringId::POSTSCRIPT_NAME));
    let prefix = first(StringId::VARIATIONS_POSTSCRIPT_NAME_PREFIX);
    let axes: Vec<(String, f32, f32)> = font
        .axes()
        .iter()
        .map(|a| (a.tag().to_string(), a.min_value(), a.max_value()))
        .collect();
    for inst in font.named_instances().iter() {
        let own = inst
            .postscript_name_id()
            .and_then(|id| font.localized_strings(id).next())
            .map(|s| s.chars().collect::<String>())
            .filter(|s| !s.is_empty());
        let derived = || {
            let sub = font
                .localized_strings(inst.subfamily_name_id())
                .next()?
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>();
            let p = prefix.as_ref()?;
            (!sub.is_empty()).then(|| format!("{p}-{sub}"))
        };
        if let Some(n) = own.or_else(derived) {
            ps.push(n);
        }
    }
    let codepoints = font.charmap().mappings().map(|(cp, _)| cp).collect();
    FontInfo {
        axes: Some(axes),
        postscript: ps,
        codepoints,
    }
}
