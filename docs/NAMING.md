# Naming Backend API

This document describes `vividarium_core::naming`, the public boundary for
taxonomy-name normalization and user-configurable Rhai hooks.

All fallible Rust functions return `CoreResult<T>`.

## Name normalization

```rust
pub fn normalize_taxonomy_name(value: &str) -> Option<String>
```

| Parameter | Description |
| --- | --- |
| `value` | Taxonomy name to normalize. |

Returns `None` for an empty value. Otherwise it trims outer whitespace,
collapses whitespace, normalizes supported quote marks, changes a standalone
hybrid `X` or multiplication sign to `x`, replaces underscores with spaces,
and converts `cv.` notation to single-quoted cultivar notation.

Formatted taxonomy input, taxonomy search, hook output, and photo mapping use
this function before comparing or storing names. Database search
normalization may additionally lowercase the result.

## Six-dimensional photo information

`TaxonomicNameInfo` contains:

| Field | Type | Description |
| --- | --- | --- |
| `family_sci` | `Option<String>` | Family scientific name. |
| `genus_sci` | `Option<String>` | Genus scientific name. |
| `species_sci` | `Option<String>` | Species scientific name. |
| `family_zh` | `Option<String>` | Family Chinese name. |
| `genus_zh` | `Option<String>` | Genus Chinese name. |
| `species_zh` | `Option<String>` | Species Chinese name. |

`ParsedPhotoFilename` contains `info: TaxonomicNameInfo` and `suffix: String`.
The suffix is the portion preserved when a taxon-derived filename is
generated, including the image extension.

```rust
pub fn default_parse_photo_filename(
    filename: &str,
) -> CoreResult<ParsedPhotoFilename>

pub fn parse_photo_filename(
    database: &Database,
    filename: &str,
) -> CoreResult<ParsedPhotoFilename>
```

| Function | Parameters | Return |
| --- | --- | --- |
| `default_parse_photo_filename` | `filename`: original filename | Parsed six-dimensional names and preserved suffix from the bundled hook. |
| `parse_photo_filename` | `database`: project database; `filename`: original filename | Parsed names and suffix from the project's effective hook. |

`default_parse_photo_filename` always executes the bundled Rhai template.
`parse_photo_filename` uses the configured photo hook when present and
otherwise executes the same template.

The bundled template treats the first unquoted digit or extension-style period
as the suffix boundary. A period followed by a space remains part of the name,
and apostrophes within one quoted name do not close that name. Legacy curly
single and double quotes become `'`. It derives family, genus, and species
scientific fields from the normalized scientific portion, including leading
hybrid genera. Chinese rank suffixes and quoted or parenthesized Chinese-name
segments may populate the Chinese fields. Applications with other filename
conventions should configure the hook.

## Synonym authority information

`ScientificNameParts` contains `name: String` and
`authority_year: Option<String>`.

```rust
pub fn default_split_scientific_name_authority(
    value: &str,
) -> CoreResult<ScientificNameParts>

pub fn split_scientific_name_authority(
    value: &str,
) -> CoreResult<ScientificNameParts>

pub fn split_scientific_name_authority_with_database(
    database: &Database,
    value: &str,
) -> CoreResult<ScientificNameParts>
```

| Function | Parameters | Return |
| --- | --- | --- |
| `default_split_scientific_name_authority` | `value`: raw scientific-name string | Name and optional authority from the bundled hook. |
| `split_scientific_name_authority` | `value`: raw scientific-name string | Same result as the bundled hook. |
| `split_scientific_name_authority_with_database` | `database`: project database; `value`: raw scientific-name string | Name and optional authority from the project's effective hook. |

The first two functions execute the bundled Rhai template. The database-aware
function uses the configured synonym hook when present and otherwise executes
the same template. Formatted updates use the database-aware behavior.

The bundled template starts authority text at the first applicable word:

1. a word containing `(`;
2. the second word whose first character is uppercase;
3. an independent `de`, `von`, or `van` word.

## Rhai hook contract

`NamingHookKind` is `photo_filename` or `synonym_authority`.

The photo hook must define:

```text
fn parse_photo_filename(filename)
```

It returns a map compatible with:

```text
#{
    info: #{
        family_sci: (),
        genus_sci: (),
        species_sci: (),
        family_zh: (),
        genus_zh: (),
        species_zh: ()
    },
    suffix: ".jpg"
}
```

Each name property may be a string or `()`. Missing properties use `()`.

The synonym hook must define:

```text
fn split_synonym_authority(value)
```

It returns:

```text
#{ name: "Canis lupus", authority_year: "Linnaeus, 1758" }
```

The `value` parameter is the exact synonym string supplied by the caller. It
is not trimmed, normalized, deduplicated, or filtered before the hook runs.
This includes empty and whitespace-only synonym entries. The bundled template
returns an empty name for those entries, which post-hook validation rejects;
a custom hook may map them to another result.
`authority_year` may be `()`. Hook name outputs always pass through
`normalize_taxonomy_name`, after which duplicate names are discarded.

Hooks have no file, network, or database functions. The engine provides
`normalize_name(value)`, `is_uppercase(character)`, and
`is_whitespace(character)` helpers used by the bundled templates.

| Helper | Parameter | Return |
| --- | --- | --- |
| `normalize_name` | `value: string` | Normalized string, or `""` for an empty name. |
| `is_uppercase` | `character: char` | Whether the character is uppercase. |
| `is_whitespace` | `character: char` | Whether the character is whitespace. |

Execution limits include 20,000 operations, 32 call levels, bounded expression
depth, 64 KiB scripts, 16 functions, 64 variables, 16 KiB strings,
64-element arrays, and 32-property maps. Script print and debug output is
discarded.

## Templates and hook settings

`NamingHookTemplates` has `photo_filename` and `synonym_authority` string
fields. `NamingHookSettings` has the same fields as optional strings; `None`
means the bundled template is active.

```rust
pub fn get_naming_hook_template(
    kind: NamingHookKind,
) -> &'static str

pub fn get_naming_hook_templates() -> NamingHookTemplates

pub fn get_naming_hook_settings(
    database: &Database,
) -> CoreResult<NamingHookSettings>

pub fn test_naming_hook(
    kind: NamingHookKind,
    script: &str,
    input: &str,
) -> CoreResult<NamingHookTestResult>
```

`NamingHookTemplates` contains the bundled `photo_filename` and
`synonym_authority` Rhai scripts. These bundled implementations run when no
successful user script exists and provide the initial editable source.

`NamingHookSettings` contains optional saved `photo_filename` and
`synonym_authority` scripts. The bundled template is the initial editor value
when no successful script exists. Hook scripts must keep executable logic
inside their hook functions instead of relying on top-level statements to
initialize scope values.

`test_naming_hook` does not save the script. Its tagged return value contains
either `ParsedPhotoFilename` or `ScientificNameParts`.

| Function | Parameters | Return |
| --- | --- | --- |
| `get_naming_hook_template` | `kind`: requested hook kind | Bundled Rhai source for that kind. |
| `get_naming_hook_templates` | none | Bundled source for both hook kinds. |
| `get_naming_hook_settings` | `database`: project database | Optional saved scripts for both hook kinds. |
| `test_naming_hook` | `kind`; unsaved `script`; raw `input` | Tagged parsed result without saving the script. |

## Project test cases

The public test models are:

| Type | Fields | Description |
| --- | --- | --- |
| `NamingHookTestResult` | tagged `kind` and `output` | A photo-filename or synonym-authority output. |
| `NamingHookTestCase` | `name`, `input`, `expected` | One named raw input and its expected tagged output. |
| `NamingHookTestCases` | `photo_filename`, `synonym_authority` | Project cases grouped by hook kind. |
| `NamingHookCaseResult` | `name`, `input`, `expected`, `actual`, `passed`, `error` | Actual outcome for one case. |
| `NamingHookTestReport` | `kind`, `passed`, `failed`, `cases` | Complete test run result. |

```rust
pub fn get_naming_hook_test_cases(
    database: &Database,
) -> CoreResult<NamingHookTestCases>

pub fn run_naming_hook_tests(
    database: &Database,
    kind: NamingHookKind,
    script: Option<&str>,
) -> CoreResult<NamingHookTestReport>

pub fn test_and_save_naming_hook(
    database: &Database,
    kind: NamingHookKind,
    script: &str,
    cases: &[NamingHookTestCase],
) -> CoreResult<NamingHookTestReport>
```

`NamingHookTestCases` contains the project test cases for both hook kinds.
Each `NamingHookTestCase` has `name`, raw `input`, and tagged `expected`
output. New projects include the bundled photo filename golden cases and
synonym-authority golden cases.

`run_naming_hook_tests` uses the supplied unsaved script when `script` is
`Some`; `None` uses the project's effective saved or bundled script. It
executes every project case in order and returns passed and failed counts.
Every `NamingHookCaseResult` includes `expected`, optional `actual`, `passed`,
and an optional execution `error`.

| Function | Parameters | Return |
| --- | --- | --- |
| `get_naming_hook_test_cases` | `database`: project database | Test cases grouped by hook kind. |
| `run_naming_hook_tests` | `database`; `kind`; optional unsaved `script` | Per-case results plus passed and failed counts. |
| `test_and_save_naming_hook` | `database`; `kind`; current `script`; ordered `cases` | Test report; saves script and cases atomically only when every case passes. |

Saving a photo filename hook queues photos in available libraries for
remapping. A failed test run leaves both the last successful script and saved
project cases unchanged.

## Desktop commands

| Command | Parameters | Return |
| --- | --- | --- |
| `normalize_taxonomy_name` | `value: string` | `string \| null` |
| `parse_photo_filename` | `filename: string` | `ParsedPhotoFilename` |
| `get_naming_hook_settings` | none | `NamingHookSettings` |
| `get_naming_hook_templates` | none | `NamingHookTemplates` |
| `test_naming_hook` | `kind: NamingHookKind`, `script: string`, `input: string` | `NamingHookTestResult` |
| `get_naming_hook_test_cases` | none | `NamingHookTestCases` |
| `run_naming_hook_tests` | `kind: NamingHookKind`, optional `script: string` | `NamingHookTestReport` |
| `test_and_save_naming_hook` | `kind: NamingHookKind`, `script: string`, `cases: NamingHookTestCase[]` | `NamingHookTestReport` |
