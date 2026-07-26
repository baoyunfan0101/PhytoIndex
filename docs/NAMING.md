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
collapses whitespace, normalizes supported apostrophes, changes a standalone
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

`default_parse_photo_filename` always uses the built-in parser.
`parse_photo_filename` uses the configured photo hook when present and
otherwise uses the built-in parser.

The built-in parser treats the first unquoted digit or extension-style period
as the suffix boundary. A period followed by a space remains part of the name.
It derives family, genus, and species scientific fields from the normalized
scientific portion. Chinese rank suffixes may populate the Chinese fields.
Applications with other filename conventions should configure the hook.

## Synonym authority information

`ScientificNameParts` contains `name: String` and
`authority_year: Option<String>`.

```rust
pub fn default_split_scientific_name_authority(
    value: &str,
) -> ScientificNameParts

pub fn split_scientific_name_authority(
    value: &str,
) -> ScientificNameParts

pub fn split_scientific_name_authority_with_database(
    database: &Database,
    value: &str,
) -> CoreResult<ScientificNameParts>
```

The first two functions expose the built-in behavior. The database-aware
function uses the configured synonym hook when present. Formatted updates use
the database-aware behavior.

The built-in parser starts authority text at the first applicable word:

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

`authority_year` may be `()`. Hook name outputs always pass through
`normalize_taxonomy_name`.

Hooks have no application-provided file, network, or database functions.
Execution limits include 20,000 operations, 32 call levels, bounded expression
depth, 64 KiB scripts, 16 functions, 64 variables, 16 KiB strings,
64-element arrays, and 32-property maps. Script print and debug output is
discarded.

## Hook settings

```rust
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

`NamingHookSettings` contains optional `photo_filename` and
`synonym_authority` scripts. Passing `None` or an empty script restores the
built-in behavior. `set_naming_hook` compiles and executes a sample before
saving. Changing the photo hook queues every photo for remapping.

`test_naming_hook` does not save the script. Its tagged return value contains
either `ParsedPhotoFilename` or `ScientificNameParts`.

## Desktop commands

| Command | Parameters | Return |
| --- | --- | --- |
| `normalize_taxonomy_name` | `value: string` | `string \| null` |
| `parse_photo_filename` | `filename: string` | `ParsedPhotoFilename` |
| `get_naming_hook_settings` | none | `NamingHookSettings` |
| `set_naming_hook` | `kind: NamingHookKind`, optional `script: string` | `null` |
| `test_naming_hook` | `kind: NamingHookKind`, `script: string`, `input: string` | `NamingHookTestResult` |
