//! Literal source search with UTF-8-safe spans for cursor placement and replacement.

use std::ops::Range;

pub fn matches(
    source: &str,
    needle: &str,
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let spans = if case_sensitive {
        source
            .match_indices(needle)
            .map(|(start, matched)| start..start + matched.len())
            .collect::<Vec<_>>()
    } else {
        // Lowercasing can change the byte length (İ, for example). Keep a mapping back to source.
        let mut folded = String::new();
        let mut mapping = Vec::new();
        for (start, character) in source.char_indices() {
            let end = start + character.len_utf8();
            for lower in character.to_lowercase() {
                mapping.push((folded.len(), start, end));
                folded.push(lower);
            }
        }
        let query = needle.to_lowercase();
        let mut spans = Vec::new();
        for (start, matched) in folded.match_indices(&query) {
            let first = mapping
                .partition_point(|entry| entry.0 <= start)
                .saturating_sub(1);
            let last = mapping
                .partition_point(|entry| entry.0 < start + matched.len())
                .saturating_sub(1);
            if let (Some(first), Some(last)) = (mapping.get(first), mapping.get(last)) {
                let span = first.1..last.2;
                if spans.last() != Some(&span) {
                    spans.push(span);
                }
            }
        }
        spans
    };
    spans
        .into_iter()
        .filter(|span| {
            !whole_word
                || (!source[..span.start]
                    .chars()
                    .next_back()
                    .is_some_and(identifier_character)
                    && !source[span.end..]
                        .chars()
                        .next()
                        .is_some_and(identifier_character))
        })
        .collect()
}

fn identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

pub fn replace_all(source: &mut String, spans: &[Range<usize>], replacement: &str) {
    for span in spans.iter().rev() {
        source.replace_range(span.clone(), replacement);
    }
}

pub fn line_start(source: &str, line: usize) -> usize {
    source
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>()
        .min(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_case_search_and_replacement_preserve_valid_source_spans() {
        let mut source = "α = İ; beta = i; % İ".to_string();
        let spans = matches(&source, "i", false, true);
        assert_eq!(spans.len(), 3);
        assert_eq!(&source[spans[0].clone()], "İ");
        replace_all(&mut source, &spans, "value");
        assert_eq!(source, "α = value; beta = value; % value");
    }

    #[test]
    fn whole_word_does_not_replace_other_identifiers_and_empty_query_is_safe() {
        let source = "value = old_value + value2 + VALUE;";
        assert_eq!(matches(source, "value", false, true).len(), 2);
        assert_eq!(matches(source, "value", true, false).len(), 3);
        assert!(matches(source, "", false, false).is_empty());
        assert_eq!(line_start("α\nb\nc", 2), 3);
        assert_eq!(line_start("α\nb\nc", 999), 6);
    }
}
