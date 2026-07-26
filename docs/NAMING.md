# Naming Backend API

This document describes `phytoindex_core::naming`, the public boundary for
taxonomy-name normalization and user-configurable Rhai hooks. Internal script
engine and metadata storage details are not part of the contract.

All fallible Rust functions return `CoreResult<T>`.

## Name normalization

```rust
pub fn normalize_taxonomy_name(value: &str) -> Option<String>
```

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

```rust
pub fn get_naming_hook_template(
    kind: NamingHookKind,
) -> &'static str

pub fn get_naming_hook_templates() -> NamingHookTemplates

pub fn get_naming_hook_settings(
    database: &Database,
) -> CoreResult<NamingHookSettings>

pub fn set_naming_hook(
    database: &Database,
    kind: NamingHookKind,
    script: Option<&str>,
) -> CoreResult<()>

pub fn test_naming_hook(
    kind: NamingHookKind,
    script: &str,
    input: &str,
) -> CoreResult<NamingHookTestResult>
```

`NamingHookTemplates` contains the bundled `photo_filename` and
`synonym_authority` Rhai scripts. These scripts are both the defaults executed
by the backend and editable starting points for users.

`NamingHookSettings` contains optional `photo_filename` and
`synonym_authority` scripts. Passing `None` or an empty script restores the
built-in template. `set_naming_hook` compiles and executes a sample before
saving. Changing the photo hook queues every photo for remapping. Operational
photo matching compiles the effective script once before the queued-photo
batch loop and reuses it across every page in that mapping run. Formatted
updates likewise compile once and reuse the parser for all rows.

Function calls use `CallFnOptions::eval_ast(false)`, so the AST is not
re-evaluated for each input. Hook scripts must therefore keep executable logic
inside their hook functions instead of relying on top-level statements to
initialize scope values.

`test_naming_hook` does not save the script. Its tagged return value contains
either `ParsedPhotoFilename` or `ScientificNameParts`.

## Project test cases

```rust
pub fn get_naming_hook_test_cases(
    database: &Database,
) -> CoreResult<NamingHookTestCases>

pub fn set_naming_hook_test_cases(
    database: &Database,
    kind: NamingHookKind,
    cases: &[NamingHookTestCase],
) -> CoreResult<()>

pub fn run_naming_hook_tests(
    database: &Database,
    kind: NamingHookKind,
    script: Option<&str>,
) -> CoreResult<NamingHookTestReport>
```

`NamingHookTestCases` contains the project test cases for both hook kinds.
Each `NamingHookTestCase` has `name`, raw `input`, and tagged `expected`
output. Cases are stored as JSON in project metadata. New projects include
the bundled photo filename golden cases and synonym-authority golden cases.

`run_naming_hook_tests` uses the supplied unsaved script when `script` is
`Some`; `None` uses the project's effective saved or default script. It
compiles that script once, executes every stored case in order, and returns
passed and failed counts. Every `NamingHookCaseResult` includes `expected`,
optional `actual`, `passed`, and an optional execution `error`.

## Desktop commands

| Command | Parameters | Return |
| --- | --- | --- |
| `normalize_taxonomy_name` | `value: string` | `string \| null` |
| `parse_photo_filename` | `filename: string` | `ParsedPhotoFilename` |
| `get_naming_hook_settings` | none | `NamingHookSettings` |
| `get_naming_hook_templates` | none | `NamingHookTemplates` |
| `set_naming_hook` | `kind: NamingHookKind`, optional `script: string` | `null` |
| `test_naming_hook` | `kind: NamingHookKind`, `script: string`, `input: string` | `NamingHookTestResult` |
| `get_naming_hook_test_cases` | none | `NamingHookTestCases` |
| `set_naming_hook_test_cases` | `kind: NamingHookKind`, `cases: NamingHookTestCase[]` | `null` |
| `run_naming_hook_tests` | `kind: NamingHookKind`, optional `script: string` | `NamingHookTestReport` |
