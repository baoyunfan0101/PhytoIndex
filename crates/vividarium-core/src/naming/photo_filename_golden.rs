use super::*;
use crate::naming::NamingHookKind;
use crate::naming::testing::{NamingHookTestResult, default_test_cases};

#[test]
fn legacy_parse_file_name_golden_cases() {
    for case in default_test_cases(NamingHookKind::PhotoFilename) {
        let NamingHookTestResult::PhotoFilename(expected) = case.expected else {
            panic!("photo filename case has the wrong output kind");
        };
        assert_eq!(
            default_parse_photo_filename(&case.input).unwrap(),
            expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn legacy_unmatched_quote_is_invalid() {
    let error = default_parse_photo_filename("Iris 'Blue030.jpg").unwrap_err();
    assert!(error.to_string().contains("unmatched quote"));
}
