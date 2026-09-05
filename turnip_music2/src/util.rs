/// We want to make sure that when native metadata isn't present, a usable sort key can still be extracted.
/// People typically number tracks e.g. 1, 2, 3,... 10? 10 should come after 9, which requires parsing multi-digit numbers.
/// This needs to handle at least two numbers - consider 'disc 1 track 23', but a more general solution is fine.
///
/// Numbers are capped at 18 digits - i64::MAX is 19 digits long so 18 will always be parseable.
/// '-1' is used for 'no-number-attached' to make sure that sorting is always consistent.
///
/// # Sorting
/// Numbers always precede letters.
/// Consider "Android 18 - Bug Song" vs "Android Attack"(?) - these should get parsed as Android 18 first, and that's fine. `["Android ", 18, " - Bug Song"]` vs `["Android Attack", -1]`.
/// More challenging: "A0" vs "A". These would parse as `[("A", 0)]` and `[("A", -1)]`, so the shorter one will go FIRST, which is correct.
/// This sorting DOES discard information - "A000000" and "A0" will sort the same, despite having different lengths.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TitleSortKey(Vec<(String, i64)>);
impl TitleSortKey {
    pub fn parse_from(mut s: &str) -> Self {
        let mut v = vec![];
        while !s.is_empty() {
            match s.find(|c: char| c.is_ascii_digit()) {
                None => {
                    v.push((s.to_string(), -1));
                    s = "";
                }
                Some(pos) => {
                    let prefix = s[..pos].to_string();
                    let digit_start = &s[pos..];
                    let digit_len = match digit_start.find(|c: char| !c.is_ascii_digit()) {
                        None => digit_start.len(),
                        Some(p) if p <= 18 => p,
                        // Limit number of digits to parse
                        Some(_) => 18,
                    };
                    let digit: i64 = digit_start[..digit_len]
                        .parse()
                        .expect("Up to 18 numerical digits will always be parseable into i64");
                    v.push((prefix, digit));
                    s = &s[pos + digit_len..];
                }
            }
        }
        Self(v)
    }
}
impl From<&str> for TitleSortKey {
    fn from(s: &str) -> Self {
        Self::parse_from(s)
    }
}

#[cfg(test)]
#[test]
fn test_title_sort_key() {
    use string_literals::s;

    assert_eq!(
        TitleSortKey::parse_from("Android 18 - Bug Song"),
        TitleSortKey(vec![(s!("Android "), 18), (s!(" - Bug Song"), -1)])
    );

    assert_eq!(
        TitleSortKey::parse_from("Android Attack"),
        TitleSortKey(vec![(s!("Android Attack"), -1)])
    );

    assert_eq!(
        TitleSortKey::parse_from("A"),
        TitleSortKey(vec![(s!("A"), -1)])
    );
    assert_eq!(
        TitleSortKey::parse_from("A0"),
        TitleSortKey(vec![(s!("A"), 0)])
    );
    assert_eq!(
        TitleSortKey::parse_from("A000000000"),
        TitleSortKey(vec![(s!("A"), 0)])
    );
}

#[cfg(test)]
#[test]
fn test_title_sort_key_sorting() {
    let mut keys = vec![
        "Android 18 - Bug Song",
        "Android Attack",
        "A000000000",
        "A",
        "A0",
    ];
    keys.sort_by_cached_key(|s| TitleSortKey::parse_from(*s));
    assert_eq!(
        keys,
        vec![
            "A",
            "A000000000", // This is 'incorrect' because all the zeros are parsed as 0, and a stable sort was used.
            "A0",
            "Android 18 - Bug Song",
            "Android Attack",
        ]
    );
}
