//! Country groups (the built-in Axis / Allies factions or user-defined ones),
//! display names and stable colours.

use crate::{data::Tag, game::CountryNames, settings::GroupSetting, theme::PALETTE};

use super::Group;

pub const AXIS: [Tag; 11] = tags([
    "GER", "ITA", "JAP", "ROM", "HUN", "SPR", "BUL", "YUG", "FIN", "MAN", "SIA",
]);
pub const ALLIES: [Tag; 13] = tags([
    "SOV", "USA", "ENG", "FRA", "CAN", "UAC", "AST", "RAJ", "SAF", "MON", "NZL", "BRA", "MEX",
]);

const fn tags<const N: usize>(names: [&str; N]) -> [Tag; N] {
    let mut out = [Tag::from_static("?"); N];
    let mut i = 0;
    while i < N {
        out[i] = Tag::from_static(names[i]);
        i += 1;
    }
    out
}

/// Name of the implicit group for countries in no other group.
pub const OTHER: &str = "Other";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupDef {
    pub name: String,
    pub tags: Vec<Tag>,
}

/// An ordered set of named country groups; countries in none of them belong
/// to [`OTHER`]. A country in several groups belongs to the first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grouping {
    defs: Vec<GroupDef>,
}

impl Grouping {
    /// Axis and Allies.
    pub fn builtin() -> Self {
        Self {
            defs: vec![
                GroupDef {
                    name: "Axis".into(),
                    tags: AXIS.to_vec(),
                },
                GroupDef {
                    name: "Allies".into(),
                    tags: ALLIES.to_vec(),
                },
            ],
        }
    }

    /// User-defined groups; unknown or malformed tags are skipped and groups
    /// without a name or countries are left out.
    pub fn custom(groups: &[GroupSetting]) -> Self {
        let defs = groups
            .iter()
            .filter_map(|g| {
                let tags = parse_tags(&g.tags);
                let name = g.name.trim();
                (!name.is_empty() && !tags.is_empty()).then(|| GroupDef {
                    name: name.to_owned(),
                    tags,
                })
            })
            .collect();
        Self { defs }
    }

    pub fn defs(&self) -> &[GroupDef] {
        &self.defs
    }

    /// Index of the group `tag` belongs to (`None` = Other).
    pub fn group_of(&self, tag: Tag) -> Option<usize> {
        self.defs.iter().position(|d| d.tags.contains(&tag))
    }

    pub fn group_name(&self, tag: Tag) -> &str {
        self.group_of(tag)
            .map_or(OTHER, |i| self.defs[i].name.as_str())
    }

    /// Available countries split into columns: one per group (in the group's
    /// own order) plus Other (alphabetical).
    pub fn lists(&self, available: impl IntoIterator<Item = Tag>) -> CountryLists {
        let mut available: Vec<Tag> = available.into_iter().collect();
        available.sort_unstable();
        available.dedup();
        let mut lists: Vec<(String, Vec<Tag>)> = self
            .defs
            .iter()
            .enumerate()
            .map(|(i, def)| {
                let tags = def
                    .tags
                    .iter()
                    .copied()
                    .filter(|t| available.binary_search(t).is_ok() && self.group_of(*t) == Some(i))
                    .collect();
                (def.name.clone(), tags)
            })
            .collect();
        let other = available
            .iter()
            .copied()
            .filter(|&t| self.group_of(t).is_none())
            .collect();
        lists.push((OTHER.to_owned(), other));
        CountryLists { lists }
    }

    /// One summed group per group that has selected countries, Other last.
    pub fn groups_for(&self, selected: &[Tag]) -> Vec<Group> {
        let mut out: Vec<Group> = self
            .defs
            .iter()
            .enumerate()
            .filter_map(|(i, def)| {
                let tags: Vec<Tag> = selected
                    .iter()
                    .copied()
                    .filter(|&t| self.group_of(t) == Some(i))
                    .collect();
                (!tags.is_empty()).then(|| Group::new(def.name.clone(), tags))
            })
            .collect();
        let other: Vec<Tag> = selected
            .iter()
            .copied()
            .filter(|&t| self.group_of(t).is_none())
            .collect();
        if !other.is_empty() {
            out.push(Group::new(OTHER.to_owned(), other));
        }
        out
    }
}

impl Default for Grouping {
    fn default() -> Self {
        Self::builtin()
    }
}

/// Tags from free text: `"GER, ita jap"` -> GER ITA JAP (deduplicated, in order).
pub fn parse_tags(text: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    for word in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let word = word.trim().to_ascii_uppercase();
        if word.bytes().all(|b| b.is_ascii_uppercase())
            && let Some(tag) = Tag::from_bytes(word.as_bytes())
            && !tags.contains(&tag)
        {
            tags.push(tag);
        }
    }
    tags
}

/// Available countries per column, as shown in the main window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CountryLists {
    lists: Vec<(String, Vec<Tag>)>,
}

impl CountryLists {
    pub fn columns(&self) -> &[(String, Vec<Tag>)] {
        &self.lists
    }

    pub fn is_empty(&self) -> bool {
        self.lists.iter().all(|(_, tags)| tags.is_empty())
    }

    /// All tags in display order.
    pub fn iter(&self) -> impl Iterator<Item = Tag> + '_ {
        self.lists.iter().flat_map(|(_, tags)| tags.iter().copied())
    }
}

/// How individual countries are labelled and coloured in graphs.
#[derive(Clone, Copy, Default)]
pub struct Labels<'a> {
    /// Use the game's country names instead of tags.
    pub names: Option<&'a CountryNames>,
    /// Same colour for a country in every graph.
    pub stable_colors: bool,
}

impl Labels<'_> {
    pub fn name(&self, tag: Tag) -> String {
        self.names
            .and_then(|n| n.get(tag))
            .map_or_else(|| tag.to_string(), str::to_owned)
    }
}

/// One group per selected country.
pub fn individual_groups(selected: &[Tag], labels: Labels<'_>) -> Vec<Group> {
    selected
        .iter()
        .map(|&tag| {
            let mut group = Group::new(labels.name(tag), vec![tag]);
            if labels.stable_colors {
                group.color = Some(stable_color(tag));
            }
            group
        })
        .collect()
}

/// A palette index that depends only on the tag. The great powers get fixed,
/// clearly distinct colours; everyone else a hash-picked one.
pub fn stable_color(tag: Tag) -> usize {
    const FIXED: [(&str, usize); 8] = [
        ("USA", 0),
        ("JAP", 1),
        ("ITA", 2),
        ("SOV", 3),
        ("FRA", 4),
        ("CHI", 5),
        ("ENG", 6),
        ("GER", 7),
    ];
    if let Some(&(_, color)) = FIXED.iter().find(|(t, _)| Tag::from_static(t) == tag) {
        return color;
    }
    // FNV-1a over the packed tag, avoiding the fixed slots where possible.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in tag.to_bits().to_be_bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    FIXED.len() + (hash % (PALETTE.len() - FIXED.len()) as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Tag {
        Tag::from_static(s)
    }

    #[test]
    fn builtin_lists_keep_canonical_order() {
        let lists =
            Grouping::builtin().lists([t("ZZZ"), t("SOV"), t("GER"), t("ITA"), t("AAA"), t("GER")]);
        let cols = lists.columns();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0], ("Axis".to_owned(), vec![t("GER"), t("ITA")]));
        assert_eq!(cols[1], ("Allies".to_owned(), vec![t("SOV")]));
        assert_eq!(cols[2], ("Other".to_owned(), vec![t("AAA"), t("ZZZ")]));
    }

    #[test]
    fn custom_groups_and_parsing() {
        let grouping = Grouping::custom(&[
            GroupSetting {
                name: " Team A ".into(),
                tags: "ger, ita;JAP GER bad1".into(),
            },
            GroupSetting {
                name: "Team B".into(),
                tags: "GER SOV".into(),
            },
            GroupSetting {
                name: "Empty".into(),
                tags: "".into(),
            },
        ]);
        assert_eq!(grouping.defs().len(), 2);
        assert_eq!(grouping.defs()[0].tags, [t("GER"), t("ITA"), t("JAP")]);
        assert_eq!(grouping.group_name(t("GER")), "Team A", "first group wins");
        assert_eq!(grouping.group_name(t("SOV")), "Team B");
        assert_eq!(grouping.group_name(t("USA")), "Other");

        let groups = grouping.groups_for(&[t("USA"), t("SOV"), t("GER"), t("ITA")]);
        let names: Vec<_> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Team A", "Team B", "Other"]);
        assert_eq!(groups[0].tags, [t("GER"), t("ITA")]);

        let lists = grouping.lists([t("GER"), t("SOV"), t("USA")]);
        let cols: Vec<_> = lists
            .columns()
            .iter()
            .map(|(n, tags)| (n.as_str(), tags.len()))
            .collect();
        assert_eq!(cols, [("Team A", 1), ("Team B", 1), ("Other", 1)]);
    }

    #[test]
    fn labels_and_colors() {
        let groups = individual_groups(&[t("GER"), t("XYZ")], Labels::default());
        assert_eq!(groups[0].name, "GER");
        assert_eq!(groups[0].color, None);
        let groups = individual_groups(
            &[t("GER"), t("XYZ")],
            Labels {
                names: None,
                stable_colors: true,
            },
        );
        assert_eq!(groups[0].color, Some(7));
        assert_eq!(groups[1].color, Some(stable_color(t("XYZ"))));
        assert!(stable_color(t("XYZ")) >= 8 && stable_color(t("XYZ")) < PALETTE.len());
    }
}
