use super::*;
use crate::naming::NamingHookKind;
use crate::naming::testing::{NamingHookTestResult, default_test_cases};

#[test]
fn synonym_authority_golden_cases() {
    for (index, case) in default_test_cases(NamingHookKind::SynonymAuthority)
        .into_iter()
        .enumerate()
    {
        let NamingHookTestResult::SynonymAuthority(expected) = case.expected else {
            panic!("synonym authority case has the wrong output kind");
        };
        assert_eq!(
            default_split_scientific_name_authority(&case.input).unwrap(),
            expected,
            "case {}",
            index + 1
        );
    }
}
