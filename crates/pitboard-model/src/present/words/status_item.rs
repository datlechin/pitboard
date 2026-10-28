//! What the menu bar's item and the notification area's icon say of the account in use: its
//! name and figure, what VoiceOver and Narrator call it, and the icon's tooltip.

use super::{PITBOARD, uncapitalised};
use unicode_segmentation::UnicodeSegmentation;

/// How many characters a person counts in `text`: extended grapheme clusters, as Swift counts
/// a `String`'s `Character`s. "é" written as "e" and a combining accent is one, and so is a
/// flag of two regional indicators or a family joined by zero-width joiners.
fn characters(text: &str) -> usize {
    text.graphemes(true).count()
}

/// A label as the menu bar has room for it: whole up to twelve characters, and past that its
/// first eleven and an ellipsis. Counted as a person counts them, so no cut falls inside an
/// accented letter, a flag or an emoji, as the Swift app's `prefix(11)` cut none.
pub(crate) fn bar_name(label: &str) -> String {
    if characters(label) <= 12 {
        return label.to_owned();
    }
    let mut cut: String = label.graphemes(true).take(11).collect();
    cut.push('…');
    cut
}

// What the menu bar calls a login signed in with no name yet.
pub(crate) const UNNAMED: &str = "unnamed";

// "work 42%".
pub(crate) fn in_bar(name: &str, figure: &str) -> String {
    format!("{name} {figure}")
}

// "Pitboard, work 42%, Codex, needs attention": the tool only after what the bar shows.
pub(crate) fn bar_spoken(shown: Option<&str>, tool: Option<&str>, attention: bool) -> String {
    let mut said = vec![PITBOARD];
    if let Some(shown) = shown {
        said.push(shown);
        said.extend(tool);
    }
    if attention {
        said.push("needs attention");
    }
    said.join(", ")
}

// The notification area's tooltip, then "work: 5-hour 12%" for each account in use.
pub(crate) fn bar_tip(
    attention: bool,
    in_use: impl IntoIterator<Item = (String, String)>,
) -> String {
    let mut tip = vec![PITBOARD.to_owned()];
    // Before the accounts, which can run past the 127 characters a tooltip holds.
    if attention {
        tip.push("Needs attention".into());
    }
    // A line goes on after its colon, in Windows' sentence case.
    tip.extend(
        in_use
            .into_iter()
            .map(|(name, line)| format!("{name}: {}", uncapitalised(&line))),
    );
    tip.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bar is shared with everything else running, so a name longer than twelve
    /// characters is cut to eleven and an ellipsis, and a name that fits is left whole.
    /// Characters as a person counts them, as Swift counts a `String`: each label here, its
    /// count and its cut are what `String.count` and `prefix(11)` gave on macOS 27.0 with
    /// Swift 6.4, written as their scalars.
    #[test]
    fn a_label_is_cut_where_swift_counts_its_characters() {
        let text = |scalars: &[u32]| -> String {
            scalars
                .iter()
                .map(|&scalar| char::from_u32(scalar).expect("a scalar"))
                .collect()
        };
        let cut_after = |label: &[u32], kept: usize| {
            let mut kept = text(&label[..kept]);
            kept.push('…');
            kept
        };
        let cases: [(&[u32], usize, Option<usize>); 22] = [
            (
                &[
                    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C,
                ],
                12,
                None,
            ),
            (
                &[
                    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D,
                ],
                13,
                Some(11),
            ),
            // café-de-la-gare, the accent combining.
            (
                &[
                    0x63, 0x61, 0x66, 0x65, 0x301, 0x2D, 0x64, 0x65, 0x2D, 0x6C, 0x61, 0x2D, 0x67,
                    0x61, 0x72, 0x65,
                ],
                15,
                Some(12),
            ),
            // Eleven flags, each two regional indicators.
            (
                &[
                    0x1F1EF, 0x1F1F5, 0x1F1EB, 0x1F1F7, 0x1F1E9, 0x1F1EA, 0x1F1EC, 0x1F1E7,
                    0x1F1EE, 0x1F1F9, 0x1F1EA, 0x1F1F8, 0x1F1FA, 0x1F1F8, 0x1F1E8, 0x1F1E6,
                    0x1F1E7, 0x1F1F7, 0x1F1F2, 0x1F1FD, 0x1F1E6, 0x1F1FA,
                ],
                11,
                None,
            ),
            // A family joined by zero-width joiners.
            (
                &[
                    0x74, 0x65, 0x61, 0x6D, 0x2D, 0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467,
                    0x200D, 0x1F466, 0x2D, 0x77, 0x6F, 0x72, 0x6B,
                ],
                11,
                None,
            ),
            // Seven thumbs with a skin tone.
            (
                &[
                    0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD,
                    0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD,
                ],
                7,
                None,
            ),
            // Seven keycaps.
            (
                &[
                    0x31, 0xFE0F, 0x20E3, 0x32, 0xFE0F, 0x20E3, 0x33, 0xFE0F, 0x20E3, 0x34, 0xFE0F,
                    0x20E3, 0x35, 0xFE0F, 0x20E3, 0x36, 0xFE0F, 0x20E3, 0x37, 0xFE0F, 0x20E3,
                ],
                7,
                None,
            ),
            // Devanagari, whose conjuncts are one character each.
            (
                &[
                    0x915, 0x94D, 0x937, 0x93F, 0x924, 0x93F, 0x91C, 0x2D, 0x915, 0x93E, 0x92E,
                    0x2D, 0x916, 0x93E, 0x924, 0x93E,
                ],
                9,
                None,
            ),
            // Hangul written in jamo, three to a syllable.
            (
                &[
                    0x1100, 0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x1100,
                    0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x2D, 0x61, 0x6E, 0x64, 0x2D, 0x6D,
                    0x6F, 0x72, 0x65,
                ],
                14,
                Some(21),
            ),
            (
                &[
                    0xD55C, 0xAD6D, 0xC5B4, 0x2D, 0xACC4, 0xC815, 0x2D, 0xBCF4, 0xC870, 0x2D,
                    0xC77C, 0xBC18,
                ],
                12,
                None,
            ),
            (
                &[
                    0x65E5, 0x672C, 0x8A9E, 0x306E, 0x30A2, 0x30AB, 0x30A6, 0x30F3, 0x30C8, 0x306E,
                    0x540D, 0x524D, 0x3067, 0x3059,
                ],
                14,
                Some(11),
            ),
            // Thirteen letters, each with two combining marks.
            (
                &[
                    0x61, 0x308, 0x301, 0x62, 0x308, 0x301, 0x63, 0x308, 0x301, 0x64, 0x308, 0x301,
                    0x65, 0x308, 0x301, 0x66, 0x308, 0x301, 0x67, 0x308, 0x301, 0x68, 0x308, 0x301,
                    0x69, 0x308, 0x301, 0x6A, 0x308, 0x301, 0x6B, 0x308, 0x301, 0x6C, 0x308, 0x301,
                    0x6D,
                ],
                13,
                Some(33),
            ),
            // A rainbow flag, joined.
            (
                &[
                    0x1F3F3, 0xFE0F, 0x200D, 0x1F308, 0x2D, 0x70, 0x72, 0x69, 0x64, 0x65, 0x2D,
                    0x74, 0x65, 0x61, 0x6D, 0x2D, 0x78,
                ],
                14,
                Some(14),
            ),
            // Scotland's flag, in tag characters.
            (
                &[
                    0x1F3F4, 0xE0067, 0xE0062, 0xE0073, 0xE0063, 0xE0074, 0xE007F, 0x2D, 0x73,
                    0x63, 0x6F, 0x74, 0x6C, 0x61, 0x6E, 0x64, 0x2D, 0x74, 0x65, 0x61, 0x6D,
                ],
                15,
                Some(17),
            ),
            // A carriage return and a line feed, one character.
            (
                &[
                    0x6C, 0x69, 0x6E, 0x65, 0xD, 0xA, 0x62, 0x72, 0x65, 0x61, 0x6B, 0x2D, 0x61,
                    0x6E, 0x64, 0x2D, 0x6D, 0x6F, 0x72, 0x65,
                ],
                19,
                Some(12),
            ),
            // Thai, whose SARA AM joins the letter before it.
            (
                &[
                    0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01,
                    0xE33, 0xE01,
                ],
                7,
                None,
            ),
            // Arabic's number sign, which joins what follows it.
            (
                &[
                    0x600, 0x31, 0x600, 0x32, 0x600, 0x33, 0x600, 0x34, 0x600, 0x35, 0x600, 0x36,
                    0x600, 0x37,
                ],
                7,
                None,
            ),
            (
                &[
                    0x646, 0x627, 0x645, 0x2D, 0x62D, 0x633, 0x627, 0x628, 0x2D, 0x637, 0x648,
                    0x64A, 0x644,
                ],
                13,
                Some(11),
            ),
            // A joiner after a letter that is not a pictograph joins nothing after it.
            (
                &[
                    0x78, 0x200D, 0x79, 0x200D, 0x7A, 0x200D, 0x77, 0x200D, 0x76, 0x200D, 0x75,
                    0x200D, 0x74, 0x2D, 0x6D, 0x6F, 0x72, 0x65,
                ],
                12,
                None,
            ),
            (
                &[
                    0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB,
                    0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB,
                    0x1F9D1, 0x200D, 0x1F4BB,
                ],
                7,
                None,
            ),
            (
                &[
                    0x915, 0x94D, 0x200D, 0x937, 0x915, 0x94D, 0x937, 0x915, 0x94D, 0x937, 0x915,
                    0x94D, 0x937, 0x915, 0x94D, 0x937,
                ],
                5,
                None,
            ),
            (
                &[
                    0x63, 0x61, 0x66, 0xE9, 0x2D, 0x64, 0x65, 0x2D, 0x6C, 0x61, 0x2D, 0x67, 0x61,
                    0x72, 0x65, 0x2D, 0x78,
                ],
                17,
                Some(11),
            ),
        ];
        for (scalars, count, kept) in cases {
            let label = text(scalars);
            assert_eq!(characters(&label), count, "{label:?}");
            let cut = kept.map_or_else(|| label.clone(), |kept| cut_after(scalars, kept));
            assert_eq!(bar_name(&label), cut, "{label:?}");
        }
    }
}
