pub fn normalize_taxonomy_name(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    for character in value.trim().chars() {
        normalized.push(match character {
            '\u{2018}' | '\u{2019}' | '\u{ff07}' => '\'',
            '\u{00d7}' => 'x',
            '_' => ' ',
            _ => character,
        });
    }
    let words = normalized
        .split_whitespace()
        .map(normalize_word)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    if let Some(index) = words.iter().position(|word| word == "cv.")
        && index > 0
        && index + 1 < words.len()
    {
        let head = words[..index].join(" ");
        let cultivar = words[index + 1..].join(" ").trim_matches('\'').to_string();
        return Some(format!("{head} '{cultivar}'"));
    }
    Some(words.join(" "))
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
            normalize_taxonomy_name("Hosta cv. Blue_Eyes"),
            Some("Hosta 'Blue Eyes'".into())
        );
        assert_eq!(normalize_taxonomy_name(" \t "), None);
    }
}
