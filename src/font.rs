//! What the index reads from a font file itself.

use std::collections::{BTreeSet, HashMap};

use crate::{MAX_NAME_RECORDS, MAX_NAMES, MAX_STRING_BYTES};

use skrifa::{FontRef, MetadataProvider, attribute::Style, raw::TableProvider, string::StringId};

#[derive(Default)]
pub struct FontInfo {
    /// `None` when the font could not be parsed (axes unknown), `Some(vec![])` for static fonts.
    pub axes: Option<Vec<(String, f32, f32)>>,
    pub postscript: Vec<String>,
    /// The style names of the face's first `MAX_NAMES` named instances (each the subfamily name
    /// as skrifa's `english_or_first` gives it, or the first non-empty record when that pick is
    /// non-empty but outside the reader's window; an instance whose pick is empty gives none),
    /// without duplicates. It may contain the own style's text (an instance named like the
    /// face's style); [`with_own_style`] removes that copy when it puts the own style first.
    /// Neither is the face's until the family name the index lists is known (see `own_style`),
    /// so a generator writes [`FontInfo::names_under`] instead: Google Fonts replaces `names`
    /// with it before it writes the record, and Noto, which names the family only once its
    /// files are read, patches the written record with [`with_own_style`].
    pub names: Vec<String>,
    /// The face's family and own style as VectorCraft names a face (`face_names`), held until the
    /// family name the index lists is known: the style and the instance names are the face's own
    /// only under that family. `None` when the font could not be parsed.
    pub own_style: Option<StyleFallback>,
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

/// A face's family and own style as VectorCraft names a face (`face_names` in its font
/// database), held until the family name the index lists is known.
#[derive(Clone, Debug)]
pub struct StyleFallback {
    /// The face's style by VectorCraft's `face_names` rule: its name ID 17 as skrifa's
    /// `english_or_first` gives it when that is non-empty, else its name ID 2 the same way, and
    /// only if its text is one of the first 64 non-empty records of its ID, the reader's
    /// window. `None` when it has none.
    pub name: Option<String>,
    /// The face's family: its name ID 16 as `english_or_first` gives it when non-empty, else its
    /// name ID 1 the same way, whole (a record is at most 64 KiB); empty when it has neither.
    pub family: String,
}

impl StyleFallback {
    /// Whether the face is a face of `family`, the family name the index lists (see
    /// [`same_family`]), so that its style and instance names are its own. A `Fam Condensed`
    /// file is not a face of `Fam`, whatever style it names.
    pub fn applies_to(&self, family: &str) -> bool {
        same_family(&self.family, family)
    }
}

impl FontInfo {
    /// The face's style names once its family name is known: nothing when the face isn't a face
    /// of `family`, else the own style first, then [`FontInfo::names`].
    pub fn names_under(&self, family: &str) -> Vec<String> {
        with_own_style(&self.names, self.own_style.as_ref(), family)
    }
}

/// `names` (the instance names) under `family`: none when the face isn't a face of `family`,
/// else with the own style first. Without a face (`None`, an unparsed font) `names` as given.
pub fn with_own_style(names: &[String], face: Option<&StyleFallback>, family: &str) -> Vec<String> {
    let Some(face) = face else {
        return names.to_vec();
    };
    if !face.applies_to(family) {
        return Vec::new();
    }
    let own = face.name.as_ref();
    own.into_iter()
        .chain(names.iter().filter(|n| Some(*n) != own))
        .cloned()
        .collect()
}

/// Whether two family names are the same: equal by letters and digits, case ignored. A name with
/// no letter or digit (or none at all) matches only by exact equality, and an absent name (the
/// empty string) never matches.
pub fn same_family(a: &str, b: &str) -> bool {
    let (sa, sb) = (squash(a), squash(b));
    if sa.is_empty() || sb.is_empty() {
        !a.is_empty() && a == b
    } else {
        sa == sb
    }
}

/// Letters and digits only, lowercased: `Fam-Black` and `fam black` are equal.
pub(crate) fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Axes, PostScript names, style names and mapped code points, read with skrifa. The PostScript
/// names are name ID 6 and, for each `fvar` named instance, its own PostScript name, or (name ID
/// 25 prefix + `-` + subfamily without spaces) when it has none.
pub fn read_font(data: &[u8]) -> FontInfo {
    let Ok(font) = FontRef::new(data) else {
        return FontInfo::default();
    };
    // One pass over the name table: for each name ID, the index of its first record. A string is
    // decoded only when an ID is asked for. Four readers, each for what an index lists:
    // `first` gives the family (`family`), designer, licence and PostScript names: the first
    // record, whole (at most 64 KiB). `english_or_first` gives the own style and the family the
    // gate compares (`face_names`'): skrifa's pick from every record, unwindowed; the style is
    // cut at `MAX_STRING_BYTES + 1` characters, the family is whole. `non_empty` is the window
    // check: the first `MAX_NAME_RECORDS` non-empty records of an ID, cut the same way, which a
    // reader accepts. `style` gives instance names: `english_or_first`, kept if it is in the
    // window, else the first non-empty record in the window.
    let name_table = font.name().ok();
    let mut records: HashMap<u16, usize> = HashMap::new();
    if let Some(name) = &name_table {
        for (i, record) in name.name_record().iter().enumerate() {
            records.entry(record.name_id().to_u16()).or_insert(i);
        }
    }
    let decode = |i: usize| {
        let name = name_table.as_ref()?;
        let record = name.name_record().get(i)?;
        Some(
            record
                .string(name.string_data())
                .ok()?
                .chars()
                .collect::<String>(),
        )
    };
    // The first record of a name ID, when it isn't empty, decoded whole: a record is at most
    // 64 KiB, and the family, designer and licence text read this way may be long.
    let first = |id: StringId| {
        records
            .get(&id.to_u16())
            .and_then(|&i| decode(i))
            .filter(|s| !s.is_empty())
    };
    let capped = |s: skrifa::string::LocalizedString<'_>| {
        s.chars().take(MAX_STRING_BYTES + 1).collect::<String>()
    };
    // The non-empty records of a name ID in table order, the first `MAX_NAME_RECORDS` of them: the
    // window in which a verifier accepts any record of the ID.
    let non_empty = |id: StringId| -> Vec<String> {
        font.localized_strings(id)
            .map(capped)
            .filter(|s| !s.is_empty())
            .take(MAX_NAME_RECORDS)
            .collect()
    };
    // A name as clients read it: skrifa's `english_or_first` (the first en-US record, else en,
    // else an unlabeled one, else the first), kept when it is non-empty. It scans the table, so it
    // can't be cut short; `cut` limits what is decoded. It is not limited to the verifier's
    // window: clients choose from every record.
    let english_or_first = |id: StringId, cut: usize| -> Option<String> {
        let name = font
            .localized_strings(id)
            .english_or_first()?
            .chars()
            .take(cut)
            .collect::<String>();
        (!name.is_empty()).then_some(name)
    };
    // An instance's style name: its `english_or_first`, and nothing when that is empty (clients
    // skip such an instance); when that pick is outside the verifier's window, the first
    // non-empty record, which is inside it.
    let style = |id: StringId| -> Option<String> {
        let pick = english_or_first(id, MAX_STRING_BYTES + 1)?;
        let all = non_empty(id);
        if all.contains(&pick) {
            Some(pick)
        } else {
            all.into_iter().next()
        }
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
    let mut names: Vec<String> = Vec::new();
    // The face's own style and family exactly as VectorCraft names a face (`face_names`), which
    // is how a client shows the face (VectorCraft may name a variable face by its default
    // instance instead): the style is `english_or_first` of ID 17 when that is non-empty, else of
    // ID 2, never the other one once chosen; the family likewise of ID 16, else ID 1, whole. Both
    // are chosen from every record. The style is listed only if its text is one of the first 64
    // non-empty records of its ID, the window a reader checks; otherwise it is not. The style and
    // the instance names are the face's own only under the family name the index lists, which
    // the generator knows and the font doesn't: a `Fam Condensed` file is no face of `Fam`.
    // Nothing else gates them.
    let own = [
        StringId::TYPOGRAPHIC_SUBFAMILY_NAME,
        StringId::SUBFAMILY_NAME,
    ]
    .into_iter()
    .find_map(|id| english_or_first(id, MAX_STRING_BYTES + 1).map(|name| (id, name)))
    .filter(|(id, name)| non_empty(*id).contains(name))
    .map(|(_, name)| name);
    let own_style = Some(StyleFallback {
        name: own,
        family: english_or_first(StringId::TYPOGRAPHIC_FAMILY_NAME, usize::MAX)
            .or_else(|| english_or_first(StringId::FAMILY_NAME, usize::MAX))
            .unwrap_or_default(),
    });
    for (i, inst) in font.named_instances().iter().enumerate() {
        // An instance's name is its first record, as in format 1.2 (read whole).
        let name = first(inst.subfamily_name_id()).unwrap_or_default();
        let coords = axes
            .iter()
            .map(|(t, _, _)| t.clone())
            .zip(inst.user_coords())
            .collect();
        // Style names come from the first `MAX_NAMES` named instances only, before repeats are
        // removed; every instance still gives its PostScript name.
        if i < MAX_NAMES
            && let Some(n) = style(inst.subfamily_name_id())
            && !names.contains(&n)
        {
            names.push(n);
        }
        instances.push((name, coords));
        let own = inst.postscript_name_id().and_then(first);
        let derived = || {
            let sub = first(inst.subfamily_name_id())?
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
        names,
        own_style,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::font::testing::{fvar, name_table, sfnt};
    use crate::{MAX_NAMES, MAX_STRING_BYTES, file_record, style_names};

    #[test]
    fn a_static_font_names_its_typographic_subfamily_else_its_subfamily() {
        let both = sfnt(&[(
            b"name",
            name_table(&[
                (1, "Fam Lumberboy"),
                (2, "Regular"),
                (16, "Fam"),
                (17, "Lumberboy"),
            ]),
        )]);
        assert_eq!(read_font(&both).names_under("Fam"), ["Lumberboy"]);
        let plain = sfnt(&[(b"name", name_table(&[(1, "Fam"), (2, "Bold Italic")]))]);
        assert_eq!(read_font(&plain).names_under("Fam"), ["Bold Italic"]);
        let none = sfnt(&[(b"name", name_table(&[(1, "Fam")]))]);
        assert!(read_font(&none).names_under("Fam").is_empty());
    }

    #[test]
    fn style_names_come_from_the_first_256_instances_only() {
        let count = MAX_NAMES + 2;
        let mut recs: Vec<(u16, String)> = vec![(1, "Fam".into()), (2, "Regular".into())];
        for i in 0..count {
            // The second and fourth instance repeat earlier names.
            let text = match i {
                1 => "Instance 0".to_string(),
                3 => "Instance 2".to_string(),
                _ => format!("Instance {i}"),
            };
            recs.push((300 + i as u16, text));
        }
        let borrowed: Vec<(u16, &str)> = recs.iter().map(|(id, t)| (*id, t.as_str())).collect();
        let coords: Vec<(u16, i32)> = (0..count).map(|i| (300 + i as u16, 400)).collect();
        let font = sfnt(&[
            (b"name", name_table(&borrowed)),
            (b"fvar", fvar(b"wght", (100, 400, 900), &coords)),
        ]);
        let info = read_font(&font);
        let names = info.names_under("Fam");
        // Own style name, then the 254 distinct names of the first 256 instances: not the 257th's.
        assert_eq!(names.len(), 1 + (MAX_NAMES - 2));
        assert!(names.contains(&format!("Instance {}", MAX_NAMES - 1)));
        assert!(!names.contains(&format!("Instance {MAX_NAMES}")));
        assert_eq!(info.instances.len(), count);
    }

    #[test]
    fn a_variable_font_adds_each_named_instance() {
        let font = sfnt(&[
            (
                b"name",
                name_table(&[
                    (1, "Fam"),
                    (2, "Regular"),
                    (256, "Weight 450"),
                    (257, "Regular"),
                    (258, "Bold"),
                ]),
            ),
            (
                b"fvar",
                fvar(
                    b"wght",
                    (100, 400, 900),
                    &[(256, 450), (257, 400), (258, 700)],
                ),
            ),
        ]);
        let info = read_font(&font);
        assert_eq!(info.names_under("Fam"), ["Regular", "Weight 450", "Bold"]);
        assert_eq!(info.instances.len(), 3);
    }

    #[test]
    fn a_file_lists_names_that_are_clean_and_bounded() {
        let long = "x".repeat(MAX_STRING_BYTES + 1);
        let given: Vec<String> = ["Weight 450", "", "Bad\u{7}", &long, "Weight 450", "Bold"]
            .map(String::from)
            .into();
        assert_eq!(style_names(&given), ["Weight 450", "Bold"]);
        let many: Vec<String> = (0..MAX_NAMES + 10).map(|i| format!("W{i}")).collect();
        assert_eq!(style_names(&many).len(), MAX_NAMES);
        let info = FontInfo {
            names: given,
            postscript: vec!["F-450".into()],
            ..FontInfo::default()
        };
        let rec = file_record("F.ttf".into(), b"x", info, "normal", 450, None);
        assert_eq!(rec["names"], json!(["Weight 450", "Bold"]));
        let bare = file_record(
            "F.ttf".into(),
            b"x",
            FontInfo::default(),
            "normal",
            400,
            None,
        );
        assert!(bare.get("names").is_none());
    }

    /// What craft-font-store's `check_names` (verify.rs, `check_names`) accepts as a style name of
    /// `font`: every non-empty record of ID 17 within the first 64, and of ID 2 only when ID 17's
    /// first or `english_or_first` record is empty, and the same of each named instance's
    /// subfamily ID among the first 256 instances. craft-font-store isn't a dev-dependency, so
    /// this mirrors its rule.
    fn verifier_accepts(data: &[u8]) -> Vec<String> {
        let font = FontRef::new(data).unwrap();
        let names = |id: StringId| -> Vec<String> {
            font.localized_strings(id)
                .map(|s| s.chars().collect::<String>())
                .filter(|s| !s.is_empty())
                .take(64)
                .collect()
        };
        let typographic = font.localized_strings(StringId::TYPOGRAPHIC_SUBFAMILY_NAME);
        let first_empty = typographic
            .clone()
            .next()
            .is_none_or(|s| s.chars().next().is_none());
        let english_empty = typographic
            .english_or_first()
            .is_some_and(|s| s.chars().next().is_none());
        let mut accepted = names(StringId::TYPOGRAPHIC_SUBFAMILY_NAME);
        if first_empty || english_empty {
            accepted.extend(names(StringId::SUBFAMILY_NAME));
        }
        for inst in font.named_instances().iter().take(256) {
            accepted.extend(names(inst.subfamily_name_id()));
        }
        accepted
    }

    #[test]
    fn a_face_is_named_as_vectorcraft_names_it_and_the_verifier_accepts_it() {
        use crate::font::testing::name_table_in as table;
        // The English ID 17 record sits after 64 others: the face is named by it, a reader can't
        // see it, so nothing is listed (and ID 2 is not used instead).
        let french: Vec<String> = (0..64).map(|i| format!("Gras {i}")).collect();
        let mut outside: Vec<(u16, u16, &str)> = vec![(1, 0x409, "Fam"), (2, 0x409, "Regular")];
        outside.extend(french.iter().map(|f| (17, 0x40c, f.as_str())));
        outside.push((17, 0x409, "Bold"));
        let fonts: Vec<(&str, Vec<u8>, &str, Vec<&str>)> = vec![
            (
                "the chosen ID 17 record is outside the reader's window",
                sfnt(&[(b"name", table(&outside))]),
                "Fam",
                vec![],
            ),
            // (what it models, font, listed family, own style expected there; empty = none)
            (
                "Tuffy: ID 17 holds translations only; VectorCraft stores the first",
                sfnt(&[(
                    b"name",
                    table(&[
                        (1, 0x409, "Tuffy"),
                        (2, 0x409, "Regular"),
                        (17, 0x40c, "NormalItalique"),
                        (17, 0x407, "StandardKursiv"),
                    ]),
                )]),
                "Tuffy",
                vec!["NormalItalique"],
            ),
            (
                "Iansui: the first family record is a translation",
                sfnt(&[(
                    b"name",
                    table(&[
                        (16, 0x404, "芫荽"),
                        (16, 0x409, "Iansui"),
                        (1, 0x409, "Iansui"),
                        (2, 0x409, "Regular"),
                    ]),
                )]),
                "Iansui",
                vec!["Regular"],
            ),
            (
                "ComicRelief: ID 2 in many languages, English wins",
                sfnt(&[(
                    b"name",
                    table(&[
                        (1, 0x409, "Comic Relief"),
                        (2, 0x403, "Negreta"),
                        (2, 0x409, "Bold"),
                    ]),
                )]),
                "Comic Relief",
                vec!["Bold"],
            ),
            (
                "Abhaya Libre: its English ID 1 names the style, not the family",
                sfnt(&[(
                    b"name",
                    table(&[(1, 0x409, "Abhaya Libre ExtraBold"), (2, 0x409, "Regular")]),
                )]),
                "Abhaya Libre",
                vec![],
            ),
            (
                "an empty English ID 17 gives way to ID 2",
                sfnt(&[(
                    b"name",
                    table(&[
                        (1, 0x409, "Fam"),
                        (2, 0x409, "Italic"),
                        (17, 0x409, ""),
                        (17, 0x404, "斜體"),
                    ]),
                )]),
                "Fam",
                vec!["Italic"],
            ),
            (
                "ID 17 in English beats ID 2",
                sfnt(&[(
                    b"name",
                    table(&[
                        (1, 0x409, "Fam Lumberboy"),
                        (2, 0x409, "Regular"),
                        (16, 0x409, "Fam"),
                        (17, 0x409, "Lumberboy"),
                    ]),
                )]),
                "Fam",
                vec!["Lumberboy"],
            ),
            (
                "a Condensed file is no face of the plain family",
                sfnt(&[(
                    b"name",
                    table(&[(16, 0x409, "Fam Condensed"), (17, 0x409, "Black")]),
                )]),
                "Fam",
                vec![],
            ),
            (
                "a variable font: instance names are accepted too",
                sfnt(&[
                    (
                        b"name",
                        table(&[
                            (1, 0x409, "Fam"),
                            (2, 0x409, "Regular"),
                            (256, 0x404, "細"),
                            (256, 0x409, "Light"),
                        ]),
                    ),
                    (b"fvar", fvar(b"wght", (100, 400, 900), &[(256, 300)])),
                ]),
                "Fam",
                vec!["Regular", "Light"],
            ),
        ];
        for (what, data, family, expected) in fonts {
            let names = read_font(&data).names_under(family);
            assert_eq!(names, expected, "{what}");
            let accepted = verifier_accepts(&data);
            for n in &names {
                assert!(accepted.contains(n), "{what}: the verifier refuses `{n}`");
            }
        }
    }

    #[test]
    fn a_choice_outside_the_readers_window_lists_nothing_and_never_the_other_id() {
        use crate::font::testing::name_table_in as table;
        // 64 French ID 17 records, then an en-US "Bold": `english_or_first` picks "Bold", which
        // a reader doesn't see (it reads the first 64 non-empty records). The face is named
        // "Bold", so ID 2's "Regular" is not its style either: nothing is listed.
        let french: Vec<String> = (0..64).map(|i| format!("Gras {i}")).collect();
        let mut recs: Vec<(u16, u16, &str)> = vec![(1, 0x409, "Fam"), (2, 0x409, "Regular")];
        recs.extend(french.iter().map(|f| (17, 0x40c, f.as_str())));
        recs.push((17, 0x409, "Bold"));
        let data = sfnt(&[(b"name", table(&recs))]);
        let info = read_font(&data);
        assert!(info.own_style.as_ref().is_some_and(|f| f.name.is_none()));
        assert!(info.names_under("Fam").is_empty());
        // Inside the window the same font lists its English style.
        let mut inside = recs.clone();
        inside.remove(2);
        let data = sfnt(&[(b"name", table(&inside))]);
        assert_eq!(read_font(&data).names_under("Fam"), ["Bold"]);
        assert!(verifier_accepts(&data).contains(&"Bold".to_string()));
    }

    #[test]
    fn a_face_family_is_chosen_from_every_record() {
        use crate::font::testing::name_table_in as table;
        // 64 French ID 16 records, then an en-US "Fam Condensed": that is the face's family
        // (what `face_names` takes), though the reader's window doesn't reach it.
        let french: Vec<String> = (0..64).map(|i| format!("Famille {i}")).collect();
        let mut recs: Vec<(u16, u16, &str)> = vec![(1, 0x409, "Fam"), (17, 0x409, "Black")];
        recs.extend(french.iter().map(|f| (16, 0x40c, f.as_str())));
        recs.push((16, 0x409, "Fam Condensed"));
        let data = sfnt(&[(b"name", table(&recs))]);
        let info = read_font(&data);
        assert!(info.names_under("Fam").is_empty());
        assert_eq!(info.names_under("Fam Condensed"), ["Black"]);
    }

    #[test]
    fn a_face_names_its_own_style_only_under_its_own_family() {
        let font = |recs: &[(u16, &str)]| read_font(&sfnt(&[(b"name", name_table(recs))]));
        // A condensed file is no face of "Fam", whether it names its style by ID 17 or ID 2.
        let condensed = font(&[
            (1, "Fam Condensed Black"),
            (16, "Fam Condensed"),
            (17, "Black"),
        ]);
        assert!(condensed.names_under("Fam").is_empty());
        assert_eq!(condensed.names_under("Fam Condensed"), ["Black"]);
        let plain = font(&[(1, "Fam Condensed"), (2, "Bold")]);
        assert!(plain.names_under("Fam").is_empty());
        assert_eq!(plain.names_under("fam-condensed"), ["Bold"]);
        // Instance names are the face's own too: none under another family.
        let variable = sfnt(&[
            (
                b"name",
                name_table(&[(1, "Fam Condensed"), (2, "Regular"), (256, "Light")]),
            ),
            (b"fvar", fvar(b"wght", (100, 400, 900), &[(256, 300)])),
        ]);
        let info = read_font(&variable);
        assert!(info.names_under("Fam").is_empty());
        assert_eq!(info.names_under("Fam Condensed"), ["Regular", "Light"]);
    }

    #[test]
    fn a_family_with_no_letters_matches_only_by_exact_equality() {
        assert!(same_family("Fam-Sans", "fam sans"));
        assert!(!same_family("Fam", "Other"));
        // No letter or digit: only the same string.
        assert!(same_family("!!", "!!"));
        assert!(!same_family("!!", "??"));
        assert!(!same_family("!!", "Fam"));
        // An absent name (no ID 1 or 16) never matches, not even the empty family.
        assert!(!same_family("", ""));
        assert!(!same_family("", "Fam"));
        let font = |recs: &[(u16, &str)]| read_font(&sfnt(&[(b"name", name_table(recs))]));
        assert!(font(&[(2, "Italic")]).names_under("").is_empty());
        assert_eq!(
            font(&[(1, "!!"), (2, "Italic")]).names_under("!!"),
            ["Italic"]
        );
    }

    #[test]
    fn an_instance_keeps_its_first_record_whole_and_a_style_name_is_its_english_pick() {
        use crate::font::testing::name_table_in as table;
        // 1.2 read an instance's name as its first record, however long; `names` takes the
        // `english_or_first` pick, capped at what a reader accepts.
        let long = "L".repeat(2000);
        let font = sfnt(&[
            (
                b"name",
                table(&[
                    (1, 0x409, "Fam"),
                    (256, 0x409, "Light"),
                    (257, 0x409, &long),
                ]),
            ),
            (
                b"fvar",
                fvar(b"wght", (100, 400, 900), &[(256, 300), (257, 400)]),
            ),
        ]);
        let info = read_font(&font);
        assert_eq!(info.instances[1].0, long);
        assert_eq!(info.names[0], "Light");
        assert_eq!(info.names[1].len(), MAX_STRING_BYTES + 1, "capped");
    }

    #[test]
    fn an_instance_whose_english_pick_is_empty_lists_nothing() {
        use crate::font::testing::name_table_in as table;
        // The en-US record is empty, a Chinese one isn't: clients skip the instance (they take
        // the pick, and an empty name names nothing), so no first-non-empty fallback.
        let font = sfnt(&[
            (
                b"name",
                table(&[
                    (1, 0x409, "Fam"),
                    (256, 0x409, ""),
                    (256, 0x404, "細"),
                    (257, 0x409, "Bold"),
                ]),
            ),
            (
                b"fvar",
                fvar(b"wght", (100, 400, 900), &[(256, 300), (257, 700)]),
            ),
        ]);
        let info = read_font(&font);
        assert_eq!(info.names, ["Bold"]);
        // A non-empty pick outside the window gives way to the first non-empty record.
        let french: Vec<String> = (0..64).map(|i| format!("Gras {i}")).collect();
        let mut recs: Vec<(u16, u16, &str)> = vec![(1, 0x409, "Fam")];
        recs.extend(french.iter().map(|f| (256, 0x40c, f.as_str())));
        recs.push((256, 0x409, "Light"));
        let font = sfnt(&[
            (b"name", table(&recs)),
            (b"fvar", fvar(b"wght", (100, 400, 900), &[(256, 300)])),
        ]);
        assert_eq!(read_font(&font).names, ["Gras 0"]);
    }

    #[test]
    fn the_gates_family_is_compared_whole() {
        use crate::font::testing::name_table_in as table;
        // "Fam" + 600 hyphens + "Condensed": cut at 513 characters it would read "Fam---..." and
        // squash to `fam`, a face of `Fam`; whole, it is `Fam Condensed`.
        let long = format!("Fam{}Condensed", "-".repeat(600));
        let font = sfnt(&[(
            b"name",
            table(&[(16, 0x409, &long), (1, 0x409, "Fam"), (17, 0x409, "Black")]),
        )]);
        let info = read_font(&font);
        assert!(info.names_under("Fam").is_empty());
        assert_eq!(info.names_under("Fam Condensed"), ["Black"]);
    }
}

/// Hand-built fonts for the unit tests.
#[cfg(test)]
pub(crate) mod testing {
    /// An sfnt with only the tables named: a table directory over the given bytes.
    pub(crate) fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0, 1, 0, 0];
        out.extend((tables.len() as u16).to_be_bytes());
        out.extend([0; 6]);
        let mut offset = 12 + 16 * tables.len();
        let mut body = Vec::new();
        for (tag, data) in tables {
            out.extend(*tag);
            out.extend([0; 4]);
            out.extend((offset as u32).to_be_bytes());
            out.extend((data.len() as u32).to_be_bytes());
            body.extend(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            offset = 12 + 16 * tables.len() + body.len();
        }
        out.extend(body);
        out
    }

    /// A `name` table (format 0, Windows Unicode English) of (name ID, string).
    pub(crate) fn name_table(records: &[(u16, &str)]) -> Vec<u8> {
        let in_english: Vec<(u16, u16, &str)> =
            records.iter().map(|&(id, s)| (id, 0x409, s)).collect();
        name_table_in(&in_english)
    }

    /// A `name` table of (name ID, Windows language ID, string); records of one ID keep their
    /// order, which is the order a reader meets them in.
    pub(crate) fn name_table_in(records: &[(u16, u16, &str)]) -> Vec<u8> {
        let mut sorted = records.to_vec();
        sorted.sort_by_key(|r| r.0);
        let mut t = vec![0, 0];
        t.extend((sorted.len() as u16).to_be_bytes());
        t.extend((6 + 12 * sorted.len() as u16).to_be_bytes());
        let mut strings = Vec::new();
        for (id, language, s) in &sorted {
            let utf16: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
            for v in [
                3u16,
                1,
                *language,
                *id,
                utf16.len() as u16,
                strings.len() as u16,
            ] {
                t.extend(v.to_be_bytes());
            }
            strings.extend(utf16);
        }
        t.extend(strings);
        t
    }

    pub(crate) fn fixed(v: i32) -> [u8; 4] {
        (v << 16).to_be_bytes()
    }

    /// An `fvar` with one axis and named instances (subfamily name ID, coordinate).
    pub(crate) fn fvar(tag: &[u8; 4], range: (i32, i32, i32), instances: &[(u16, i32)]) -> Vec<u8> {
        let mut t = Vec::new();
        for v in [1u16, 0, 16, 2, 1, 20, instances.len() as u16, 8] {
            t.extend(v.to_be_bytes());
        }
        t.extend(tag);
        for v in [range.0, range.1, range.2] {
            t.extend(fixed(v));
        }
        t.extend([0, 0, 1, 0]);
        for (name, coord) in instances {
            t.extend(name.to_be_bytes());
            t.extend([0, 0]);
            t.extend(fixed(*coord));
        }
        t
    }
}
