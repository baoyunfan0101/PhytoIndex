use tempfile::TempDir;

use super::*;

fn database() -> (TempDir, Database) {
    let directory = TempDir::new().unwrap();
    let database = Database::open(directory.path().join("test.db")).unwrap();
    (directory, database)
}

fn validate_parentage(rows: &[(i64, Option<i64>, i64)]) -> CoreResult<()> {
    let (_directory, database) = database();
    let connection = database.connect_taxonomy_metadata_context()?;
    connection.execute_batch("PRAGMA foreign_keys = OFF")?;
    for (taxon_id, parent_taxon_id, rank) in rows {
        connection.execute(
            "INSERT INTO taxa (taxon_id, parent_taxon_id, rank) VALUES (?, ?, ?)",
            params![taxon_id, parent_taxon_id, rank],
        )?;
        connection.execute(
            "INSERT INTO taxon_names (taxon_id, name_type, name) VALUES (?, 1, ?)",
            params![taxon_id, format!("Taxon {taxon_id}")],
        )?;
    }
    validate_taxonomy(&connection)
}

#[test]
fn parentage_validation_accepts_the_complete_five_rank_tree() {
    validate_parentage(&[
        (1, None, 1),
        (2, Some(1), 2),
        (3, Some(2), 3),
        (4, Some(3), 4),
        (5, Some(4), 5),
    ])
    .unwrap();
}

#[test]
fn parentage_validation_accepts_skipped_ranks() {
    validate_parentage(&[
        (1, None, 1),
        (2, Some(1), 2),
        (3, Some(1), 3),
        (4, Some(1), 4),
        (5, Some(2), 5),
        (6, Some(3), 5),
    ])
    .unwrap();
}

#[test]
fn parentage_validation_rejects_equal_parent_and_child_ranks() {
    let error = validate_parentage(&[(1, None, 1), (2, Some(1), 2), (3, Some(2), 2)]).unwrap_err();
    assert!(error.to_string().contains("taxon 3 has invalid parentage"));
}

#[test]
fn parentage_validation_rejects_a_lower_rank_parent() {
    let error = validate_parentage(&[(1, None, 1), (2, Some(1), 5), (3, Some(2), 3)]).unwrap_err();
    assert!(error.to_string().contains("taxon 3 has invalid parentage"));
}

#[test]
fn parentage_validation_rejects_a_missing_parent() {
    let error = validate_parentage(&[(1, None, 1), (2, Some(99), 2)]).unwrap_err();
    assert!(error.to_string().contains("taxon 2 has invalid parentage"));
}

#[test]
fn parentage_validation_rejects_a_cycle() {
    let error = validate_parentage(&[(1, Some(2), 2), (2, Some(1), 3)]).unwrap_err();
    assert!(error.to_string().contains("invalid parentage"));
}

#[test]
fn parentage_validation_rejects_a_parentless_non_kingdom() {
    let error = validate_parentage(&[(1, None, 1), (2, None, 3)]).unwrap_err();
    assert!(error.to_string().contains("taxon 2 has invalid parentage"));
}

#[test]
fn parentage_validation_rejects_a_kingdom_with_a_parent() {
    let error = validate_parentage(&[(1, Some(2), 1), (2, None, 1)]).unwrap_err();
    assert!(error.to_string().contains("taxon 1 has invalid parentage"));
}

#[test]
fn name_type_codes_follow_public_name_order() {
    for (index, name_type) in TaxonomyNameType::ALL.into_iter().enumerate() {
        let code = index as i64 + 1;
        assert_eq!(name_type.code(), code);
        assert_eq!(TaxonomyNameType::from_code(code).unwrap(), name_type);
    }
    assert!(TaxonomyNameType::from_code(0).is_err());
    assert!(TaxonomyNameType::from_code(7).is_err());
}

#[test]
fn formatted_update_uses_the_configured_synonym_hook() {
    let (_directory, database) = database();
    crate::naming::set_naming_hook(
        &database,
        crate::naming::NamingHookKind::SynonymAuthority,
        Some(
            r#"
                fn split_synonym_authority(value) {
                    if value == "  Raw_synonym  " {
                        #{ name: value, authority_year: "raw" }
                    } else if value == "   " {
                        #{ name: "Whitespace input", authority_year: "raw" }
                    } else {
                        #{ name: "Wrong input", authority_year: "normalized" }
                    }
                }
                "#,
        ),
    )
    .unwrap();
    let rows = parse_taxonomy_input_csv(
        &database,
        "kingdom|synonyms\nAnimalia|  Raw_synonym  ;   \n",
    )
    .unwrap();
    assert_eq!(rows[0].synonyms, vec!["  Raw_synonym  ", "   "]);
    apply_rows(&database, &rows).unwrap();
    let connection = database.connect_taxonomy_metadata_context().unwrap();
    let mut statement = connection
        .prepare("SELECT name, authority_year FROM taxon_names WHERE name_type = 2 ORDER BY name")
        .unwrap();
    let names = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        names,
        vec![
            ("Raw synonym".into(), "raw".into()),
            ("Whitespace input".into(), "raw".into()),
        ]
    );
}

#[test]
fn empty_synonym_cell_is_an_empty_list() {
    let (_directory, database) = database();
    let rows = parse_taxonomy_input_csv(&database, "kingdom|synonyms\nAnimalia|\n").unwrap();

    assert!(rows[0].synonyms.is_empty());
}

#[test]
fn csv_accepts_subset_and_reordered_headers() {
    let (_directory, database) = database();
    let rows = parse_taxonomy_input_csv(
        &database,
        "synonyms|species|zh_alias\nCanis lycaon Linnaeus, 1758|Canis lupus|wolf;dog\n",
    )
    .unwrap();
    assert_eq!(rows[0].species.as_deref(), Some("Canis lupus"));
    assert_eq!(rows[0].synonyms.len(), 1);
    assert_eq!(rows[0].zh_alias, vec!["wolf", "dog"]);
}

#[test]
fn csv_rejects_rows_with_a_different_column_count() {
    let (_directory, database) = database();
    let error = parse_taxonomy_input_csv(&database, "kingdom|order\nAnimalia\n").unwrap_err();
    assert!(error.to_string().contains("fields"));
}

#[test]
fn preview_rolls_back_and_apply_is_revertible() {
    let (_directory, database) = database();
    let input = TaxonInputRow {
        kingdom: Some("Animalia".into()),
        ..TaxonInputRow::default()
    };
    assert_eq!(
        preview_rows(&database, std::slice::from_ref(&input))
            .unwrap()
            .rows[0]
            .operation_types,
        vec![TaxonRowStatus::NewTaxon, TaxonRowStatus::NewName]
    );
    assert!(
        database
            .connect_taxonomy_metadata_context()
            .unwrap()
            .query_row("SELECT NOT EXISTS(SELECT 1 FROM taxa)", [], |row| row
                .get::<_, bool>(0),)
            .unwrap()
    );
    let result = apply_rows(&database, &[input]).unwrap();
    rollback_operation(&database, result.operation_id).unwrap();
    assert!(
        operations::get_operation(
            &database.connect_taxonomy_metadata_context().unwrap(),
            result.operation_id,
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn one_row_reports_every_change_type() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            authority_year: Some("old".into()),
            geological_range: Some("old".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let connection = database.connect_taxonomy_metadata_context().unwrap();
    connection
        .execute(
            "UPDATE taxon_names SET source = NULL WHERE name_type = 1",
            [],
        )
        .unwrap();
    drop(connection);

    let preview = preview_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            authority_year: Some("new".into()),
            synonyms: vec!["Metazoa Linnaeus, 1758".into()],
            geological_range: Some("new".into()),
            source: Some("catalog".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();

    assert_eq!(
        preview.rows[0].operation_types,
        vec![
            TaxonRowStatus::Supplement,
            TaxonRowStatus::NewName,
            TaxonRowStatus::Overwrite
        ]
    );
}

#[test]
fn source_only_fills_an_empty_existing_value() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            source: Some("first".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let result = apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            source: Some("second".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert_eq!(
        result.rows[0].operation_types,
        vec![TaxonRowStatus::NoChange]
    );
    let source: String = database
        .connect_taxonomy_metadata_context()
        .unwrap()
        .query_row(
            "SELECT source FROM taxon_names WHERE name_type = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source, "first");
}

#[test]
fn row_source_applies_to_supplied_lineage_names() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let result = apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            order: Some("Carnivora".into()),
            source: Some("catalog".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert_eq!(
        result.rows[0].operation_types,
        vec![
            TaxonRowStatus::NewTaxon,
            TaxonRowStatus::Supplement,
            TaxonRowStatus::NewName
        ]
    );
    let source: String = database
        .connect_taxonomy_metadata_context()
        .unwrap()
        .query_row(
            "SELECT source FROM taxon_names WHERE name = 'Animalia'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source, "catalog");
}

#[test]
fn matched_synonym_receives_its_authority_and_other_names_become_synonyms() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Accepted name".into()),
            synonyms: vec!["Matched synonym".into()],
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();

    let result = apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Proposed name".into()),
            authority_year: Some("Proposed authority".into()),
            synonyms: vec![
                "Matched synonym Matched authority".into(),
                "Other synonym Other authority".into(),
            ],
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();

    assert_eq!(
        result.rows[0].operation_types,
        vec![TaxonRowStatus::Supplement, TaxonRowStatus::NewName]
    );
    let connection = database.connect_taxonomy_metadata_context().unwrap();
    let names = connection
        .prepare(
            r#"
                SELECT name_type, name, authority_year
                FROM taxon_names
                ORDER BY name_id
                "#,
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        names,
        vec![
            (
                TaxonomyNameType::SciName.code(),
                "Accepted name".into(),
                None
            ),
            (
                TaxonomyNameType::Synonym.code(),
                "Matched synonym".into(),
                Some("Matched authority".into())
            ),
            (
                TaxonomyNameType::Synonym.code(),
                "Proposed name".into(),
                Some("Proposed authority".into())
            ),
            (
                TaxonomyNameType::Synonym.code(),
                "Other synonym".into(),
                Some("Other authority".into())
            ),
        ]
    );
}

#[test]
fn input_priority_precedes_database_name_type_priority() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[
            TaxonInputRow {
                kingdom: Some("First taxon".into()),
                synonyms: vec!["First input".into()],
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Second input".into()),
                ..TaxonInputRow::default()
            },
        ],
    )
    .unwrap();

    let result = apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("No match".into()),
            synonyms: vec!["First input".into(), "Second input".into()],
            geological_range: Some("selected".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert_eq!(
        result.rows[0]
            .target
            .as_ref()
            .and_then(|target| target.names.sci_name.as_deref()),
        Some("First taxon")
    );
}

#[test]
fn existing_sci_name_precedes_existing_synonym_for_one_input_name() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[
            TaxonInputRow {
                kingdom: Some("Shared".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Other".into()),
                ..TaxonInputRow::default()
            },
        ],
    )
    .unwrap();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Other".into()),
            synonyms: vec!["Shared".into()],
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();

    let preview = preview_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Shared".into()),
            geological_range: Some("selected".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert_eq!(
        preview.rows[0]
            .target
            .as_ref()
            .and_then(|target| target.names.sci_name.as_deref()),
        Some("Shared")
    );
    assert!(
        !preview.rows[0]
            .operation_types
            .contains(&TaxonRowStatus::MultipleCandidates)
    );
}

#[test]
fn sparse_lineage_fields_are_valid_filters_and_direct_parent_input() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                genus: Some("Canis".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                genus: Some("Canis".into()),
                species: Some("Canis lupus".into()),
                ..TaxonInputRow::default()
            },
        ],
    )
    .unwrap();

    let sparse_match = preview_rows(
        &database,
        &[TaxonInputRow {
            family: Some("Canidae".into()),
            species: Some("Canis lupus".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert_eq!(
        sparse_match.rows[0]
            .target
            .as_ref()
            .and_then(|target| target.names.sci_name.as_deref()),
        Some("Canis lupus")
    );
    assert_eq!(
        sparse_match.rows[0].operation_types,
        vec![TaxonRowStatus::NoChange]
    );

    let direct_parent_only = apply_rows(
        &database,
        &[TaxonInputRow {
            order: Some("Carnivora".into()),
            family: Some("Felidae".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    assert!(
        direct_parent_only.rows[0]
            .operation_types
            .contains(&TaxonRowStatus::NewTaxon)
    );
    assert_eq!(
        direct_parent_only.rows[0]
            .parent
            .as_ref()
            .and_then(|parent| parent.names.sci_name.as_deref()),
        Some("Carnivora")
    );
}
