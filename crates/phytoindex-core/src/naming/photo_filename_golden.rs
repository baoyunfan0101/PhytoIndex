use super::*;

struct GoldenCase {
    label: &'static str,
    input: &'static str,
    info: TaxonomicNameInfo,
    suffix: &'static str,
}

fn scientific(
    family: Option<&str>,
    genus: Option<&str>,
    species: Option<&str>,
) -> TaxonomicNameInfo {
    TaxonomicNameInfo {
        family_sci: family.map(str::to_owned),
        genus_sci: genus.map(str::to_owned),
        species_sci: species.map(str::to_owned),
        ..TaxonomicNameInfo::default()
    }
}

#[test]
fn legacy_parse_file_name_golden_cases() {
    let cases = [
        GoldenCase {
            label: "family suffix",
            input: "Herbertaceae003.jpg",
            info: scientific(Some("Herbertaceae"), None, None),
            suffix: "003.jpg",
        },
        GoldenCase {
            label: "genus",
            input: "Herbertus005.jpg",
            info: scientific(None, Some("Herbertus"), None),
            suffix: "005.jpg",
        },
        GoldenCase {
            label: "species",
            input: "Herbertus dicranus010.jpg",
            info: scientific(None, Some("Herbertus"), Some("Herbertus dicranus")),
            suffix: "010.jpg",
        },
        GoldenCase {
            label: "quoted internal apostrophe",
            input: "Iris 'a'b'030.jpg",
            info: scientific(None, Some("Iris"), Some("Iris 'a'b'")),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "curly double quotes",
            input: "Iris \u{201c}Blue\u{201d}030.jpg",
            info: scientific(None, Some("Iris"), Some("Iris 'Blue'")),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "cultivar conversion",
            input: "Hosta cv. blue_eyes030.jpg",
            info: scientific(None, Some("Hosta 'Blue"), Some("Hosta 'Blue Eyes'")),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "sex marker",
            input: "Herbertus dicranusM010.jpg",
            info: scientific(None, Some("Herbertus"), Some("Herbertus dicranus")),
            suffix: "M010.jpg",
        },
        GoldenCase {
            label: "doubtful marker",
            input: "Herbertus dicranusYN010.jpg",
            info: scientific(None, Some("Herbertus"), Some("Herbertus dicranus")),
            suffix: "YN010.jpg",
        },
        GoldenCase {
            label: "leading hybrid genus",
            input: "\u{00d7} Gasteraloe030.jpg",
            info: scientific(None, Some("x Gasteraloe"), None),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "leading hybrid species",
            input: "\u{00d7} Gasteraloe beguinii030.jpg",
            info: scientific(None, Some("x Gasteraloe"), Some("x Gasteraloe beguinii")),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "infix hybrid species",
            input: "Pinus X pekinensis030.jpg",
            info: scientific(None, Some("Pinus x"), Some("Pinus x pekinensis")),
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "family genus species Chinese",
            input: "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}\u{79cd} Canis lupus020.jpg",
            info: TaxonomicNameInfo {
                family_sci: None,
                genus_sci: Some("Canis".into()),
                species_sci: Some("Canis lupus".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}\u{79cd}".into()),
            },
            suffix: "020.jpg",
        },
        GoldenCase {
            label: "ke ke shu exception",
            input: "\u{9999}\u{79d1}\u{79d1}\u{5c5e}020.jpg",
            info: TaxonomicNameInfo {
                genus_zh: Some("\u{9999}\u{79d1}\u{79d1}\u{5c5e}".into()),
                ..TaxonomicNameInfo::default()
            },
            suffix: "020.jpg",
        },
        GoldenCase {
            label: "quoted ASCII inside Chinese species",
            input: "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}'abc' Gasteraloe 'Wonder'030.jpg",
            info: TaxonomicNameInfo {
                family_sci: None,
                genus_sci: Some("Gasteraloe".into()),
                species_sci: Some("Gasteraloe 'Wonder'".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}'abc'".into()),
            },
            suffix: "030.jpg",
        },
        GoldenCase {
            label: "parenthesized ASCII inside Chinese species",
            input: "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}(abc) Gasteraloe beguinii030.jpg",
            info: TaxonomicNameInfo {
                family_sci: None,
                genus_sci: Some("Gasteraloe".into()),
                species_sci: Some("Gasteraloe beguinii".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}(abc)".into()),
            },
            suffix: "030.jpg",
        },
    ];

    for case in cases {
        assert_eq!(
            default_parse_photo_filename(case.input).unwrap(),
            ParsedPhotoFilename {
                info: case.info,
                suffix: case.suffix.into(),
            },
            "{}",
            case.label
        );
    }
}

#[test]
fn legacy_unmatched_quote_is_invalid() {
    let error = default_parse_photo_filename("Iris 'Blue030.jpg").unwrap_err();
    assert!(error.to_string().contains("unmatched quote"));
}
