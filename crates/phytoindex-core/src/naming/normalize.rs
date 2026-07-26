pub fn normalize_taxonomy_name(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    for character in value.trim().chars() {
        normalized.push(match character {
            '\u{2018}' | '\u{2019}' | '\u{201c}' | '\u{201d}' | '\u{ff07}' => '\'',
            '\u{00d7}' => 'x',
            _ => character,
        });
    }
    let normalized = normalize_cv_notation(&normalized).replace('_', " ");
    let words = normalized
        .split_whitespace()
        .map(normalize_word)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    Some(words.join(" "))
}

fn normalize_cv_notation(value: &str) -> String {
    let Some(marker) = value.find(" cv. ") else {
        return value.to_owned();
    };
    let cultivar_start = marker + " cv. ".len();
    let cultivar_end = value[cultivar_start..]
        .find(char::is_whitespace)
        .map_or(value.len(), |index| cultivar_start + index);
    if cultivar_start == cultivar_end {
        return value.to_owned();
    }
    let cultivar = value[cultivar_start..cultivar_end]
        .split('_')
        .map(uppercase_first)
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}'{cultivar}'{}",
        &value[..marker + 1],
        &value[cultivar_end..]
    )
}

fn uppercase_first(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect::<String>()
}

fn normalize_word(value: &str) -> String {
    if value == "X" {
        return "x".into();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spacing_quotes_hybrids_and_underscores() {
        assert_eq!(
            normalize_taxonomy_name("  Pinus  \u{00d7}  alba  "),
            Some("Pinus x alba".into())
        );
        assert_eq!(
            normalize_taxonomy_name("Hosta \u{2018}Blue_Eyes\u{2019}"),
            Some("Hosta 'Blue Eyes'".into())
        );
        assert_eq!(
            normalize_taxonomy_name("Hosta \u{201c}Blue_Eyes\u{201d}"),
            Some("Hosta 'Blue Eyes'".into())
        );
        assert_eq!(
            normalize_taxonomy_name("Hosta cv. blue_eyes f. alba"),
            Some("Hosta 'Blue Eyes' f. alba".into())
        );
        assert_eq!(normalize_taxonomy_name(" \t "), None);
    }
}
