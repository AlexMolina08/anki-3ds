// Copyright: Anki 3DS contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Preserve upstream-rendered cloze emphasis through plain-text conversion.

pub const CLOZE_START: char = '\u{e000}';
pub const CLOZE_END: char = '\u{e001}';

pub fn plain(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(*ch, CLOZE_START | CLOZE_END))
        .collect()
}

/// Only Anki's generated cloze span changes color; other HTML remains intact
/// for the backend's existing HTML/entity conversion. Track nested spans so a
/// formatting span inside a cloze cannot end its emphasis early.
pub fn mark_clozes(html: &str) -> Option<String> {
    let mut output = String::new();
    let mut spans = Vec::new();
    let mut remaining = html;
    let mut found = false;
    while let Some(start) = remaining.find('<') {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let mut quote = None;
        let end = remaining.char_indices().find_map(|(index, ch)| {
            if let Some(delimiter) = quote {
                if ch == delimiter {
                    quote = None;
                }
            } else if ch == '\'' || ch == '"' {
                quote = Some(ch);
            } else if ch == '>' {
                return Some(index + 1);
            }
            None
        });
        let Some(end) = end else { break };
        let tag = &remaining[..end];
        if tag.starts_with("</span") {
            if spans.pop().unwrap_or(false) {
                output.push(CLOZE_END);
            }
            output.push_str(tag);
        } else if tag.starts_with("<span ") || tag == "<span>" {
            // This exact class attribute is emitted by upstream cloze
            // rendering.
            let cloze = tag.starts_with("<span class=\"cloze\"");
            spans.push(cloze);
            output.push_str(tag);
            if cloze {
                output.push(CLOZE_START);
                found = true;
            }
        } else {
            output.push_str(tag);
        }
        remaining = &remaining[end..];
    }
    output.push_str(remaining);
    found.then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_nested_markup_and_multiple_upstream_clozes() {
        let html = "A <span class=\"cloze\" data-ordinal=\"1\">blue <span style=\"x>y\">nested</span></span> and <span class=\"cloze\" data-ordinal=\"1\">two</span>";
        let marked = mark_clozes(html).unwrap();
        assert_eq!(plain(&marked), html);
        assert!(marked.contains("\u{e000}blue <span style=\"x>y\">nested</span>\u{e001}</span>"));
        assert_eq!(marked.matches(CLOZE_START).count(), 2);
        assert_eq!(marked.matches(CLOZE_END).count(), 2);
    }

    #[test]
    fn basic_cards_do_not_change() {
        assert!(mark_clozes("plain <span>text</span>").is_none());
        assert!(mark_clozes("<span class=\"not-cloze\">x</span>").is_none());
    }
}
