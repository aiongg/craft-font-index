//! What the index reads from a font file itself.

use std::collections::BTreeSet;

use skrifa::{FontRef, MetadataProvider, attribute::Style, string::StringId};

#[derive(Default)]
pub struct FontInfo {
    /// `None` when the font could not be parsed (axes unknown), `Some(vec![])` for static fonts.
    pub axes: Option<Vec<(String, f32, f32)>>,
    pub postscript: Vec<String>,
    /// The `fvar` named instances: subfamily name and the user coordinate of every axis, in
    /// axis order.
    pub instances: Vec<(String, Vec<(String, f32)>)>,
    /// Mapped code points, for language coverage.
    pub codepoints: BTreeSet<u32>,
    /// Typographic family name (name ID 16), else name ID 1.
    pub family: Option<String>,
    /// Name ID 9.
    pub designer: Option<String>,
    /// Name IDs 13 and 14, joined.
    pub license: String,
    /// The default instance's weight: the `wght` axis default, else OS/2 usWeightClass.
    pub weight: f32,
    /// The default instance's width in percent: the `wdth` axis default, else OS/2 usWidthClass.
    pub width: f32,
    pub italic: bool,
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
    let mut instances = Vec::new();
    for inst in font.named_instances().iter() {
        let name = font
            .localized_strings(inst.subfamily_name_id())
            .next()
            .map(|s| s.chars().collect::<String>())
            .unwrap_or_default();
        let coords = axes
            .iter()
            .map(|(t, _, _)| t.clone())
            .zip(inst.user_coords())
            .collect();
        instances.push((name, coords));
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
    let attrs = font.attributes();
    let axis_default = |tag: &[u8; 4]| {
        font.axes()
            .iter()
            .find(|a| a.tag() == skrifa::Tag::new(tag))
            .map(|a| a.default_value())
    };
    let license = [StringId::LICENSE_DESCRIPTION, StringId::LICENSE_URL]
        .into_iter()
        .filter_map(first)
        .collect::<Vec<_>>()
        .join("\n");
    FontInfo {
        axes: Some(axes),
        postscript: ps,
        instances,
        codepoints,
        family: first(StringId::TYPOGRAPHIC_FAMILY_NAME).or_else(|| first(StringId::FAMILY_NAME)),
        designer: first(StringId::DESIGNER),
        license,
        weight: axis_default(b"wght").unwrap_or(attrs.weight.value()),
        width: axis_default(b"wdth").unwrap_or(attrs.stretch.percentage()),
        italic: attrs.style != Style::Normal,
    }
}
