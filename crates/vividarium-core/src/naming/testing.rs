use serde::{Deserialize, Serialize};

use super::{
    NamingHookKind, ParsedPhotoFilename, PhotoFilenameParser, ScientificNameParts,
    SynonymAuthorityParser, TaxonomicNameInfo,
};
use crate::metadata::{self, MetadataKey};
use crate::{CoreError, CoreResult, Database};

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
    let connection = database.connect_metadata()?;
    Ok(NamingHookTestCases {
        photo_filename: load_cases(&connection, NamingHookKind::PhotoFilename)?,
        synonym_authority: load_cases(&connection, NamingHookKind::SynonymAuthority)?,
    })
}

#[cfg(test)]
pub(crate) fn set_naming_hook_test_cases(
    database: &Database,
    kind: NamingHookKind,
    cases: &[NamingHookTestCase],
) -> CoreResult<()> {
    validate_cases(kind, cases)?;
    metadata::set_json(
        &database.connect_metadata()?,
        tests_metadata_key(kind),
        &cases,
    )
}

pub fn test_naming_hook(
    kind: NamingHookKind,
    script: &str,
    input: &str,
) -> CoreResult<NamingHookTestResult> {
    HookRunner::from_script(kind, script)?.run(input)
}

pub fn run_naming_hook_tests(
    kind: NamingHookKind,
    script: &str,
    cases: &[NamingHookTestCase],
) -> CoreResult<NamingHookTestReport> {
    if script.trim().is_empty() {
        return Err(CoreError::InvalidArgument(
            "naming hook script is required".into(),
        ));
    }
    validate_cases(kind, cases)?;
    run_cases(kind, HookRunner::from_script(kind, script)?, cases.to_vec())
}

pub fn save_naming_hook(
    database: &Database,
    kind: NamingHookKind,
    script: &str,
    cases: &[NamingHookTestCase],
) -> CoreResult<()> {
    if script.trim().is_empty() {
        return Err(CoreError::InvalidArgument(
            "naming hook script is required".into(),
        ));
    }
    validate_cases(kind, cases)?;
    HookRunner::from_script(kind, script)?;
    let mut connection = database.connect_metadata()?;
    let transaction = connection.transaction()?;
    metadata::set_raw(&transaction, kind.metadata_key(), script)?;
    metadata::set_json(&transaction, tests_metadata_key(kind), &cases)?;
    transaction.commit()?;
    let _ = super::queue_photo_hook_remap(database, kind);
    Ok(())
}

fn run_cases(
    kind: NamingHookKind,
    runner: HookRunner,
    cases: Vec<NamingHookTestCase>,
) -> CoreResult<NamingHookTestReport> {
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
                    input: case.input,
                    expected: case.expected,
                    actual: Some(actual),
                    passed: matches,
                    error: None,
                }
            }
            Err(error) => NamingHookCaseResult {
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
                "Canis lupus (Linnaeus, 1758)",
                "Canis lupus",
                "(Linnaeus, 1758)",
            ),
            synonym_authority_test_case(
                "Canis lupus de Silva, 1900",
                "Canis lupus",
                "de Silva, 1900",
            ),
            synonym_authority_test_case(
                "\u{200c}Paidia moabitica de Freina, 2004",
                "\u{200c}Paidia moabitica",
                "de Freina, 2004",
            ),
            synonym_authority_test_case(
                "\u{200c}Sedum eriocarpum subsp. spathulifolium 't Hart, 1995",
                "\u{200c}Sedum eriocarpum subsp. spathulifolium",
                "'t Hart, 1995",
            ),
            synonym_authority_test_case("Sedum fragrans 't Hart", "Sedum fragrans", "'t Hart"),
            synonym_authority_test_case(
                "Hippocampus natalensis von Bonde, 1923",
                "Hippocampus natalensis",
                "von Bonde, 1923",
            ),
            synonym_authority_test_case(
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
            "Herbertaceae003.jpg",
            TaxonomicNameInfo {
                family_sci: Some("Herbertaceae".into()),
                ..TaxonomicNameInfo::default()
            },
            "003.jpg",
        ),
        photo_filename_test_case(
            "Herbertus005.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                ..TaxonomicNameInfo::default()
            },
            "005.jpg",
        ),
        photo_filename_test_case(
            "Herbertus dicranus010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "010.jpg",
        ),
        photo_filename_test_case(
            "Iris 'a'b'030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Iris".into()),
                species_sci: Some("Iris 'a'b'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "Iris \u{201c}Blue\u{201d}030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Iris".into()),
                species_sci: Some("Iris 'Blue'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "Hosta cv. blue_eyes030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Hosta 'Blue".into()),
                species_sci: Some("Hosta 'Blue Eyes'".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "Herbertus dicranusM010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "M010.jpg",
        ),
        photo_filename_test_case(
            "Herbertus dicranusYN010.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Herbertus".into()),
                species_sci: Some("Herbertus dicranus".into()),
                ..TaxonomicNameInfo::default()
            },
            "YN010.jpg",
        ),
        photo_filename_test_case(
            "\u{00d7} Gasteraloe030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("x Gasteraloe".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "\u{00d7} Gasteraloe beguinii030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("x Gasteraloe".into()),
                species_sci: Some("x Gasteraloe beguinii".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
            "Pinus X pekinensis030.jpg",
            TaxonomicNameInfo {
                genus_sci: Some("Pinus x".into()),
                species_sci: Some("Pinus x pekinensis".into()),
                ..TaxonomicNameInfo::default()
            },
            "030.jpg",
        ),
        photo_filename_test_case(
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
            "\u{9999}\u{79d1}\u{79d1}\u{5c5e}020.jpg",
            TaxonomicNameInfo {
                genus_zh: Some("\u{9999}\u{79d1}\u{79d1}\u{5c5e}".into()),
                ..TaxonomicNameInfo::default()
            },
            "020.jpg",
        ),
        photo_filename_test_case(
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
    input: &str,
    info: TaxonomicNameInfo,
    suffix: &str,
) -> NamingHookTestCase {
    NamingHookTestCase {
        input: input.into(),
        expected: NamingHookTestResult::PhotoFilename(ParsedPhotoFilename {
            info,
            suffix: suffix.into(),
        }),
    }
}

fn synonym_authority_test_case(
    input: &str,
    name: &str,
    authority_year: &str,
) -> NamingHookTestCase {
    NamingHookTestCase {
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
        metadata::insert_json_if_missing(
            connection,
            tests_metadata_key(kind),
            &default_test_cases(kind),
        )?;
    }
    Ok(())
}

fn load_cases(
    connection: &rusqlite::Connection,
    kind: NamingHookKind,
) -> CoreResult<Vec<NamingHookTestCase>> {
    let cases = metadata::get_json(connection, tests_metadata_key(kind))?
        .unwrap_or_else(|| default_test_cases(kind));
    validate_cases(kind, &cases)?;
    Ok(cases)
}

fn validate_cases(kind: NamingHookKind, cases: &[NamingHookTestCase]) -> CoreResult<()> {
    for (index, case) in cases.iter().enumerate() {
        if case.expected.kind() != kind {
            return Err(CoreError::InvalidArgument(format!(
                "hook test {} expected output has the wrong kind",
                index + 1
            )));
        }
    }
    Ok(())
}

fn tests_metadata_key(kind: NamingHookKind) -> MetadataKey {
    match kind {
        NamingHookKind::PhotoFilename => MetadataKey::PhotoFilenameHookTests,
        NamingHookKind::SynonymAuthority => MetadataKey::SynonymAuthorityHookTests,
    }
}

enum HookRunner {
    PhotoFilename(PhotoFilenameParser),
    SynonymAuthority(SynonymAuthorityParser),
}

impl HookRunner {
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
            let cases = default_test_cases(kind);
            let expected_count = cases.len();
            let report =
                run_naming_hook_tests(kind, get_naming_hook_template(kind), &cases).unwrap();
            assert_eq!(report.failed, 0);
            assert_eq!(report.passed, expected_count);
            assert!(report.cases.iter().all(|case| case.actual.is_some()));
        }

        let saved = get_naming_hook_test_cases(&database).unwrap();
        assert_eq!(saved.photo_filename.len(), 15);
        assert_eq!(saved.synonym_authority.len(), 7);
    }

    #[test]
    fn ignores_legacy_test_names_when_loading_metadata() {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let expected = default_test_cases(NamingHookKind::SynonymAuthority)
            .into_iter()
            .next()
            .unwrap();
        let mut stored = serde_json::to_value(vec![expected.clone()]).unwrap();
        stored[0]["name"] = serde_json::Value::String("legacy label".into());
        metadata::set_raw(
            &database.connect_metadata().unwrap(),
            tests_metadata_key(NamingHookKind::SynonymAuthority),
            &serde_json::to_string(&stored).unwrap(),
        )
        .unwrap();

        let loaded = get_naming_hook_test_cases(&database).unwrap();
        assert_eq!(loaded.synonym_authority, vec![expected]);
    }

    #[test]
    fn saves_project_test_cases_and_reports_actual_failures() {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let cases = vec![NamingHookTestCase {
            input: "Canidae.jpg".into(),
            expected: NamingHookTestResult::PhotoFilename(ParsedPhotoFilename {
                info: TaxonomicNameInfo::default(),
                suffix: ".jpg".into(),
            }),
        }];
        set_naming_hook_test_cases(&database, NamingHookKind::PhotoFilename, &cases).unwrap();

        let saved = get_naming_hook_test_cases(&database).unwrap();
        assert_eq!(saved.photo_filename, cases);
        let report = run_naming_hook_tests(
            NamingHookKind::PhotoFilename,
            get_naming_hook_template(NamingHookKind::PhotoFilename),
            &cases,
        )
        .unwrap();
        assert_eq!(report.passed, 0);
        assert_eq!(report.failed, 1);
        assert!(report.cases[0].actual.is_some());
        assert!(!report.cases[0].passed);
    }

    #[test]
    fn test_and_save_are_independent() {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let script = get_naming_hook_template(NamingHookKind::SynonymAuthority);
        let mut cases = default_test_cases(NamingHookKind::SynonymAuthority);
        cases[0].expected = NamingHookTestResult::SynonymAuthority(ScientificNameParts {
            name: "wrong".into(),
            authority_year: None,
        });

        let failed =
            run_naming_hook_tests(NamingHookKind::SynonymAuthority, script, &cases).unwrap();
        assert_eq!(failed.failed, 1);
        assert_eq!(
            crate::naming::get_naming_hook_settings(&database)
                .unwrap()
                .synonym_authority,
            None
        );

        let cases = default_test_cases(NamingHookKind::SynonymAuthority);
        let passed =
            run_naming_hook_tests(NamingHookKind::SynonymAuthority, script, &cases).unwrap();
        assert_eq!(passed.failed, 0);
        assert_eq!(
            crate::naming::get_naming_hook_settings(&database)
                .unwrap()
                .synonym_authority,
            None
        );

        save_naming_hook(&database, NamingHookKind::SynonymAuthority, script, &cases).unwrap();
        assert_eq!(
            crate::naming::get_naming_hook_settings(&database)
                .unwrap()
                .synonym_authority,
            Some(script.to_string())
        );
        assert_eq!(
            get_naming_hook_test_cases(&database)
                .unwrap()
                .synonym_authority,
            cases
        );
    }
}
