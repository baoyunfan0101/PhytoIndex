use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{
    NamingHookKind, ParsedPhotoFilename, PhotoFilenameParser, ScientificNameParts,
    SynonymAuthorityParser, TaxonomicNameInfo,
};
use crate::{CoreError, CoreResult, Database};

pub(crate) const PHOTO_FILENAME_TESTS_KEY: &str = "photo_filename_hook_tests";
pub(crate) const SYNONYM_AUTHORITY_TESTS_KEY: &str = "synonym_authority_hook_tests";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "output", rename_all = "snake_case")]
pub enum NamingHookTestResult {
    PhotoFilename(ParsedPhotoFilename),
    SynonymAuthority(ScientificNameParts),
}

impl NamingHookTestResult {
    fn kind(&self) -> NamingHookKind {
        match self {
            Self::PhotoFilename(_) => NamingHookKind::PhotoFilename,
            Self::SynonymAuthority(_) => NamingHookKind::SynonymAuthority,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookTestCase {
    pub name: String,
    pub input: String,
    pub expected: NamingHookTestResult,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookTestCases {
    pub photo_filename: Vec<NamingHookTestCase>,
    pub synonym_authority: Vec<NamingHookTestCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookCaseResult {
    pub name: String,
    pub input: String,
    pub expected: NamingHookTestResult,
    pub actual: Option<NamingHookTestResult>,
    pub passed: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookTestReport {
    pub kind: NamingHookKind,
    pub passed: usize,
    pub failed: usize,
    pub cases: Vec<NamingHookCaseResult>,
}

pub fn get_naming_hook_test_cases(database: &Database) -> CoreResult<NamingHookTestCases> {
    let connection = database.connect()?;
    Ok(NamingHookTestCases {
        photo_filename: load_cases(&connection, NamingHookKind::PhotoFilename)?,
        synonym_authority: load_cases(&connection, NamingHookKind::SynonymAuthority)?,
    })
}

pub fn set_naming_hook_test_cases(
    database: &Database,
    kind: NamingHookKind,
    cases: &[NamingHookTestCase],
) -> CoreResult<()> {
    validate_cases(kind, cases)?;
    let value = serde_json::to_string(cases)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid hook tests: {error}")))?;
    database.connect()?.execute(
        r#"
        INSERT INTO app_metadata (metadata_key, metadata_value)
        VALUES (?, ?)
        ON CONFLICT(metadata_key) DO UPDATE
        SET metadata_value = excluded.metadata_value
        "#,
        params![tests_metadata_key(kind), value],
    )?;
    Ok(())
}

pub fn test_naming_hook(
    kind: NamingHookKind,
    script: &str,
    input: &str,
) -> CoreResult<NamingHookTestResult> {
    HookRunner::from_script(kind, script)?.run(input)
}

pub fn run_naming_hook_tests(
    database: &Database,
    kind: NamingHookKind,
    script: Option<&str>,
) -> CoreResult<NamingHookTestReport> {
    let connection = database.connect()?;
    let cases = load_cases(&connection, kind)?;
    let runner = match script {
        Some(script) => HookRunner::from_script(kind, script)?,
        None => HookRunner::load(&connection, kind)?,
    };
    let mut passed = 0;
    let results = cases
        .into_iter()
        .map(|case| match runner.run(&case.input) {
            Ok(actual) => {
                let matches = actual == case.expected;
                if matches {
                    passed += 1;
                }
                NamingHookCaseResult {
                    name: case.name,
                    input: case.input,
                    expected: case.expected,
                    actual: Some(actual),
                    passed: matches,
                    error: None,
                }
            }
            Err(error) => NamingHookCaseResult {
                name: case.name,
                input: case.input,
                expected: case.expected,
                actual: None,
                passed: false,
                error: Some(error.to_string()),
            },
        })
        .collect::<Vec<_>>();
    Ok(NamingHookTestReport {
        kind,
        passed,
        failed: results.len() - passed,
        cases: results,
    })
}

pub(crate) fn default_test_cases(kind: NamingHookKind) -> Vec<NamingHookTestCase> {
    match kind {
        NamingHookKind::PhotoFilename => default_photo_filename_test_cases(),
        NamingHookKind::SynonymAuthority => vec![
            synonym_authority_test_case(
                "parenthesized authority",
                "Canis lupus (Linnaeus, 1758)",
                "Canis lupus",
                "(Linnaeus, 1758)",
            ),
            synonym_authority_test_case(
                "lowercase authority prefix",
                "Canis lupus de Silva, 1900",
                "Canis lupus",
                "de Silva, 1900",
            ),
            synonym_authority_test_case(
                "de authority",
                "\u{200c}Paidia moabitica de Freina, 2004",
                "\u{200c}Paidia moabitica",
                "de Freina, 2004",
            ),
            synonym_authority_test_case(
                "apostrophe authority with year",
                "\u{200c}Sedum eriocarpum subsp. spathulifolium 't Hart, 1995",
                "\u{200c}Sedum eriocarpum subsp. spathulifolium",
                "'t Hart, 1995",
            ),
            synonym_authority_test_case(
                "apostrophe authority",
                "Sedum fragrans 't Hart",
                "Sedum fragrans",
                "'t Hart",
            ),
            synonym_authority_test_case(
                "von authority",
                "Hippocampus natalensis von Bonde, 1923",
                "Hippocampus natalensis",
                "von Bonde, 1923",
            ),
            synonym_authority_test_case(
                "van authority",
                "Hylophilus moxensis van Els, T. Wijpkema, J.T. Wijpkema, Avalos & Montenegro-Avila, 2026",
                "Hylophilus moxensis",
                "van Els, T. Wijpkema, J.T. Wijpkema, Avalos & Montenegro-Avila, 2026",
            ),
        ],
    }
}

fn default_photo_filename_test_cases() -> Vec<NamingHookTestCase> {
    vec![
        photo_filename_test_case(
            "family suffix",
            "Herbertaceae003.jpg",
            TaxonomicNameInfo {
                family_sci: Some("Herbertaceae".into()),
                ..TaxonomicNameInfo::default()
            },
            "003.jpg",
        ),
        photo_filename_test_case(
            "genus",
            "Herbertus005.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                ..TaxonomicNameInfo::default()
            },
            "005.jpg",
        ),
        photo_filename_test_case(
            "species",
            "Herbertus dicranus010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "010.jpg",
        ),
        photo_filename_test_case(
            "quoted internal apostrophe",
            "Iris 'a'b'030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Iris".into()),
                species_sci: Some("Iris 'a'b'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "curly double quotes",
            "Iris \u{201c}Blue\u{201d}030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Iris".into()),
                species_sci: Some("Iris 'Blue'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "cultivar conversion",
            "Hosta cv. blue_eyes030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Hosta 'Blue".into()),
                species_sci: Some("Hosta 'Blue Eyes'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "sex marker",
            "Herbertus dicranusM010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "M010.jpg",
        ),
        photo_filename_test_case(
            "doubtful marker",
            "Herbertus dicranusYN010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "YN010.jpg",
        ),
        photo_filename_test_case(
            "leading hybrid genus",
            "\u{00d7} Gasteraloe030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("x Gasteraloe".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "leading hybrid species",
            "\u{00d7} Gasteraloe beguinii030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("x Gasteraloe".into()),
                species_sci: Some("x Gasteraloe beguinii".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "infix hybrid species",
            "Pinus X pekinensis030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Pinus x".into()),
                species_sci: Some("Pinus x pekinensis".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "family genus species Chinese",
            "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}\u{79cd} Canis lupus020.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Canis".into()),
                species_sci: Some("Canis lupus".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}\u{79cd}".into()),
                ..TaxonomicNameInfo::default()
            },
            "020.jpg",
        ),
        photo_filename_test_case(
            "ke ke shu exception",
            "\u{9999}\u{79d1}\u{79d1}\u{5c5e}020.jpg",
            TaxonomicNameInfo {
                genus_zh: Some("\u{9999}\u{79d1}\u{79d1}\u{5c5e}".into()),
                ..TaxonomicNameInfo::default()
            },
            "020.jpg",
        ),
        photo_filename_test_case(
            "quoted ASCII inside Chinese species",
            "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}'abc' Gasteraloe 'Wonder'030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Gasteraloe".into()),
                species_sci: Some("Gasteraloe 'Wonder'".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}'abc'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "parenthesized ASCII inside Chinese species",
            "\u{9999}\u{79d1}\u{9999}\u{5c5e}\u{9999}(abc) Gasteraloe beguinii030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Gasteraloe".into()),
                species_sci: Some("Gasteraloe beguinii".into()),
                family_zh: Some("\u{9999}\u{79d1}".into()),
                genus_zh: Some("\u{9999}\u{5c5e}".into()),
                species_zh: Some("\u{9999}(abc)".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
    ]
}

fn photo_filename_test_case(
    name: &str,
    input: &str,
    info: TaxonomicNameInfo,
    suffix: &str,
) -> NamingHookTestCase {
    NamingHookTestCase {
        name: name.into(),
        input: input.into(),
        expected: NamingHookTestResult::PhotoFilename(ParsedPhotoFilename {
            info,
            suffix: suffix.into(),
        }),
    }
}

fn synonym_authority_test_case(
    case_name: &str,
    input: &str,
    name: &str,
    authority_year: &str,
) -> NamingHookTestCase {
    NamingHookTestCase {
        name: case_name.into(),
        input: input.into(),
        expected: NamingHookTestResult::SynonymAuthority(ScientificNameParts {
            name: name.into(),
            authority_year: Some(authority_year.into()),
        }),
    }
}

pub(crate) fn seed_default_test_cases(connection: &rusqlite::Connection) -> CoreResult<()> {
    for kind in [
        NamingHookKind::PhotoFilename,
        NamingHookKind::SynonymAuthority,
    ] {
        let value = serde_json::to_string(&default_test_cases(kind))
            .map_err(|error| CoreError::InvalidArgument(format!("invalid hook tests: {error}")))?;
        connection.execute(
            r#"
            INSERT INTO app_metadata (metadata_key, metadata_value)
            VALUES (?, ?)
            ON CONFLICT(metadata_key) DO NOTHING
            "#,
            params![tests_metadata_key(kind), value],
        )?;
    }
    Ok(())
}

fn load_cases(
    connection: &rusqlite::Connection,
    kind: NamingHookKind,
) -> CoreResult<Vec<NamingHookTestCase>> {
    let value = connection
        .query_row(
            "SELECT metadata_value FROM app_metadata WHERE metadata_key = ?",
            [tests_metadata_key(kind)],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let cases = match value {
        Some(value) => serde_json::from_str(&value)
            .map_err(|error| CoreError::InvalidArgument(format!("invalid hook tests: {error}")))?,
        None => default_test_cases(kind),
    };
    validate_cases(kind, &cases)?;
    Ok(cases)
}

fn validate_cases(kind: NamingHookKind, cases: &[NamingHookTestCase]) -> CoreResult<()> {
    for (index, case) in cases.iter().enumerate() {
        if case.name.trim().is_empty() {
            return Err(CoreError::InvalidArgument(format!(
                "hook test {} requires a name",
                index + 1
            )));
        }
        if case.expected.kind() != kind {
            return Err(CoreError::InvalidArgument(format!(
                "hook test {} expected output has the wrong kind",
                index + 1
            )));
        }
    }
    Ok(())
}

fn tests_metadata_key(kind: NamingHookKind) -> &'static str {
    match kind {
        NamingHookKind::PhotoFilename => PHOTO_FILENAME_TESTS_KEY,
        NamingHookKind::SynonymAuthority => SYNONYM_AUTHORITY_TESTS_KEY,
    }
}

enum HookRunner {
    PhotoFilename(PhotoFilenameParser),
    SynonymAuthority(SynonymAuthorityParser),
}

impl HookRunner {
    fn load(connection: &rusqlite::Connection, kind: NamingHookKind) -> CoreResult<Self> {
        match kind {
            NamingHookKind::PhotoFilename => {
                Ok(Self::PhotoFilename(PhotoFilenameParser::load(connection)?))
            }
            NamingHookKind::SynonymAuthority => Ok(Self::SynonymAuthority(
                SynonymAuthorityParser::load(connection)?,
            )),
        }
    }

    fn from_script(kind: NamingHookKind, script: &str) -> CoreResult<Self> {
        match kind {
            NamingHookKind::PhotoFilename => Ok(Self::PhotoFilename(
                PhotoFilenameParser::from_script(script)?,
            )),
            NamingHookKind::SynonymAuthority => Ok(Self::SynonymAuthority(
                SynonymAuthorityParser::from_script(script)?,
            )),
        }
    }

    fn run(&self, input: &str) -> CoreResult<NamingHookTestResult> {
        match self {
            Self::PhotoFilename(parser) => {
                Ok(NamingHookTestResult::PhotoFilename(parser.parse(input)?))
            }
            Self::SynonymAuthority(parser) => {
                Ok(NamingHookTestResult::SynonymAuthority(parser.split(input)?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::naming::get_naming_hook_template;

    #[test]
    fn runs_default_metadata_cases_as_one_batch() {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();

        for kind in [
            NamingHookKind::PhotoFilename,
            NamingHookKind::SynonymAuthority,
        ] {
            let expected_count = default_test_cases(kind).len();
            let report =
                run_naming_hook_tests(&database, kind, Some(get_naming_hook_template(kind)))
                    .unwrap();
            assert_eq!(report.failed, 0);
            assert_eq!(report.passed, expected_count);
            assert!(report.cases.iter().all(|case| case.actual.is_some()));
        }

        let saved = get_naming_hook_test_cases(&database).unwrap();
        assert_eq!(saved.photo_filename.len(), 15);
        assert_eq!(saved.synonym_authority.len(), 7);
    }

    #[test]
    fn saves_project_test_cases_and_reports_actual_failures() {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let cases = vec![NamingHookTestCase {
            name: "expected mismatch".into(),
            input: "Canidae.jpg".into(),
            expected: NamingHookTestResult::PhotoFilename(ParsedPhotoFilename {
                info: TaxonomicNameInfo::default(),
                suffix: ".jpg".into(),
            }),
        }];
        set_naming_hook_test_cases(&database, NamingHookKind::PhotoFilename, &cases).unwrap();

        let saved = get_naming_hook_test_cases(&database).unwrap();
        assert_eq!(saved.photo_filename, cases);
        let report = run_naming_hook_tests(&database, NamingHookKind::PhotoFilename, None).unwrap();
        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 1);
        assert!(report.cases[0].actual.is_some());
        assert!(!report.cases[0].passed);
    }
}
