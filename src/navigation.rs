//! UTF-8-safe text utilities for Quark's editor controls.

fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

pub fn line_col(text: &str, cursor: usize) -> (usize, usize) {
    let before = &text[..char_to_byte(text, cursor)];
    (
        before.bytes().filter(|byte| *byte == b'\n').count() + 1,
        before.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    )
}

pub fn line_start(text: &str, line: usize) -> Option<usize> {
    match line {
        0 => None,
        1 => Some(0),
        n => text
            .match_indices('\n')
            .nth(n - 2)
            .map(|(byte, _)| text[..byte + 1].chars().count()),
    }
}

pub fn find(text: &str, pattern: &str, cursor: usize, reverse: bool) -> Option<(usize, usize)> {
    if pattern.is_empty() {
        return None;
    }
    let hits: Vec<_> = text
        .match_indices(pattern)
        .map(|(byte, found)| {
            let start = text[..byte].chars().count();
            (start, start + found.chars().count())
        })
        .collect();
    if reverse {
        hits.iter()
            .rev()
            .copied()
            .find(|(start, _)| *start < cursor)
            .or_else(|| hits.last().copied())
    } else {
        hits.iter()
            .copied()
            .find(|(start, _)| *start >= cursor)
            .or_else(|| hits.first().copied())
    }
}

pub fn duplicate_line(text: &mut String, cursor: usize) -> usize {
    let byte = char_to_byte(text, cursor);
    let start = text[..byte].rfind('\n').map_or(0, |byte| byte + 1);
    let end = text[byte..]
        .find('\n')
        .map_or(text.len(), |offset| byte + offset + 1);
    let line = text[start..end].to_owned();
    let column = text[start..byte].chars().count();
    let insertion = if line.ends_with('\n') {
        text.insert_str(end, &line);
        end
    } else {
        text.push('\n');
        let new_start = text.len();
        text.push_str(&line);
        new_start
    };
    text[..insertion].chars().count() + column
}

pub fn replace_at(text: &mut String, range: (usize, usize), replacement: &str) -> usize {
    let start = char_to_byte(text, range.0);
    let end = char_to_byte(text, range.1);
    text.replace_range(start..end, replacement);
    range.0 + replacement.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_uses_char_indices() {
        let s = "π\nabc\n";
        assert_eq!(line_col(s, 2), (2, 1));
        assert_eq!(line_start(s, 3), Some(6));
        assert_eq!(line_start(s, 4), None);
    }
    #[test]
    fn search_wraps_and_supports_unicode() {
        let s = "π abc π abc";
        assert_eq!(find(s, "abc", 0, false), Some((2, 5)));
        assert_eq!(find(s, "abc", 5, false), Some((8, 11)));
        assert_eq!(find(s, "abc", 11, false), Some((2, 5)));
        assert_eq!(find(s, "abc", 0, true), Some((8, 11)));
        assert_eq!(find(s, "", 0, false), None);
    }
    #[test]
    fn duplication_respects_crlf() {
        let mut s = "a\r\nπx\r\nz".to_owned();
        let cursor = duplicate_line(&mut s, 4);
        assert_eq!(s, "a\r\nπx\r\nπx\r\nz");
        assert_eq!(line_col(&s, cursor), (3, 2));
    }
    #[test]
    fn duplicate_last_and_replace() {
        let mut s = "a\nπx".to_owned();
        let cursor = duplicate_line(&mut s, 3);
        assert_eq!(s, "a\nπx\nπx");
        assert_eq!(line_col(&s, cursor), (3, 2));
        let hit = find(&s, "π", 4, false).unwrap();
        let end = replace_at(&mut s, hit, "✓✓");
        assert_eq!(s, "a\nπx\n✓✓x");
        assert_eq!(line_col(&s, end), (3, 3));
    }
}
