//! Allocation-free text helpers shared by the scrapers and parsers.

/// Case-insensitive ASCII substring search.
///
/// The obvious implementation — `haystack.to_lowercase().contains(needle)` —
/// allocates and copies the *entire* haystack. On a scrape that is a full copy
/// of the response body (hundreds of KB) performed once per marker test, plus a
/// `char`-wise Unicode lowercase pass. This scans in place instead: one byte
/// compare to find a candidate start, then a slice compare only at candidates.
///
/// ASCII-only by design: every marker this is used for is ASCII, and upstream
/// HTML markers are ASCII in practice.
pub fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();

    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    let first = needle[0].to_ascii_lowercase();
    let last_start = haystack.len() - needle.len();
    let mut i = 0;
    while i <= last_start {
        if haystack[i].to_ascii_lowercase() == first
            && haystack[i..i + needle.len()].eq_ignore_ascii_case(needle)
        {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ignoring_ascii_case() {
        assert!(contains_ignore_ascii_case("Hello World", "hello"));
        assert!(contains_ignore_ascii_case("Hello World", "WORLD"));
        assert!(contains_ignore_ascii_case("a", "a"));
    }

    #[test]
    fn rejects_absent_or_oversized_needles() {
        assert!(!contains_ignore_ascii_case("Hello World", "hello!"));
        assert!(!contains_ignore_ascii_case("x", "longer than haystack"));
        assert!(!contains_ignore_ascii_case("", "x"));
        // An empty needle is vacuously present; callers never pass one.
        assert!(contains_ignore_ascii_case("", ""));
    }

    #[test]
    fn handles_non_ascii_haystack_without_panicking() {
        // Multi-byte UTF-8 must not be split at a char boundary: the scan works
        // on raw bytes, so it simply never matches inside a code point.
        assert!(contains_ignore_ascii_case(
            "Anime Terbaik — 2024",
            "terbaik"
        ));
        assert!(!contains_ignore_ascii_case("Anime Terbaik — 2024", "komik"));
    }
}
