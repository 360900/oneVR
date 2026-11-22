//! Turning the desktop keyboard's edits into what OpenVR apps see. Used by the
//! `xrizer-keyboard` executable; the library only tests it.

/// The keys that turn `old` into `new` for an app that only sees keys: backspaces back to where
/// they differ, then the rest of `new`.
pub fn keys_for_edit(old: &str, new: &str) -> String {
    // Both strings have the same bytes up to here, so it's a char boundary of each.
    let common = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((i, _), _)| i);
    let erased = old[common..].chars().count();
    let mut keys = "\x08".repeat(erased);
    keys.push_str(&new[common..]);
    keys
}

/// `text` cut to at most `max` characters, 0 being no limit.
pub fn limit_chars(text: &str, max: u32) -> &str {
    match text.char_indices().nth(max as usize) {
        Some((end, _)) if max > 0 => &text[..end],
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_become_keys() {
        assert_eq!(keys_for_edit("", "abc"), "abc");
        assert_eq!(keys_for_edit("abc", "abcd"), "d");
        assert_eq!(keys_for_edit("abc", "ab"), "\x08");
        assert_eq!(keys_for_edit("abc", ""), "\x08\x08\x08");
        assert_eq!(keys_for_edit("abc", "abc"), "");
        // An edit in the middle erases back to it and types the rest again.
        assert_eq!(keys_for_edit("abc", "aXbc"), "\x08\x08Xbc");
        // Backspaces count characters, not bytes.
        assert_eq!(keys_for_edit("aéé", "a"), "\x08\x08");
        assert_eq!(keys_for_edit("é", "è"), "\x08è");
    }

    #[test]
    fn limits_count_characters() {
        assert_eq!(limit_chars("abcdef", 3), "abc");
        assert_eq!(limit_chars("ééé", 2), "éé");
        assert_eq!(limit_chars("ab", 3), "ab");
        assert_eq!(limit_chars("abc", 0), "abc");
    }
}
