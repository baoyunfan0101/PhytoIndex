# Operation and Audit Backend API

`vividarium_core::operations` defines the DTOs shared by photo rename and
taxonomy history. Domain functions are exported by
`vividarium_core::photos` and `vividarium_core::taxonomy`.

## `OperationSummary`

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Domain-local operation identity. |
| `kind` | `String` | Operation kind. |
| `source` | `String` | User workflow that created the operation. |
| `applied_at` | `String` | Apply timestamp. |
| `total_items` | `usize` | Total audit units. |
| `succeeded_items` | `usize` | Successful units. |
| `failed_items` | `usize` | Failed units. |
| `rollbackable` | `bool` | Whether rollback is currently supported. |
| `has_formatted_input` | `bool` | Whether taxonomy formatted input is available. |

History list interfaces return summaries only. They never include complete
audit or formatted input rows.

## `OperationAuditRow`

| Field | Type | Description |
| --- | --- | --- |
| `operation_id` | `i64` | Parent operation. |
| `sequence` | `usize` | One-based order within the operation. |
| `entity_type` | `String` | Changed entity category. |
| `entity_id` | `Option<String>` | Stable entity identity when available. |
| `action` | `String` | Performed or attempted action. |
| `before_json` | `Option<Value>` | Relevant state before the action. |
| `after_json` | `Option<Value>` | Relevant state after the action. |
| `succeeded` | `bool` | Whether this audit row succeeded. |
| `message` | `String` | Result or failure explanation. |

Photo rename rows use `entity_type = "photo"` and `action = "rename"`.
Their JSON state contains `directory_relative_path` and `filename`.
Taxonomy rows use the same fields for taxon and taxon-name changes.

## `OperationPage<T>`

| Field | Type | Description |
| --- | --- | --- |
| `items` | `Vec<T>` | Current page. |
| `next_cursor` | `Option<String>` | Opaque next-page cursor. |

Pass `None` for the first page. A cursor is scoped to its interface,
operation, and domain. Limits are clamped to `1..=500`.

## Domain interfaces

Both `photos` and `taxonomy` expose:

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `list_operations` | `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationSummary>` |
| `list_operation_audit` | `operation_id: i64`, `cursor: Option<&str>`, `limit: usize` | `OperationPage<OperationAuditRow>` |
| `write_operation_audit` | `operation_id: i64`, `writer: &mut W` where `W: Write` | `()` |
| `write_operations_audit` | `operation_ids: &[i64]`, `writer: &mut W` where `W: Write` | `()` |
| `write_all_operation_audit` | `writer: &mut W` where `W: Write` | `()` |
| `rollback_operation` | `operation_id: i64` | `()` |

Audit CSV columns are identical in both domains:

```text
operation_id,sequence,entity_type,entity_id,action,before_json,after_json,succeeded,message
```

Rows stream directly from the database to the caller-provided writer in
operation and sequence order. Batch and all-history exports use one header;
the complete CSV is never materialized by the export interface. The delimiter
is comma by default and follows the application-wide General setting; supported
values are comma, semicolon, tab, and pipe.

Successful rollback deletes the original operation and its audit data.
Taxonomy rollback also applies its reverse changeset and records pending
photo-library synchronization. No user-visible reverse operation is created.

## Taxonomy formatted input

Only formatted updates and direct UI edits converted to formatted updates set
`has_formatted_input`.

The taxonomy module additionally exposes:

| Function | Parameters after `database` | Return |
| --- | --- | --- |
| `export_operation_input` | `operation_id: i64` | Formatted-update CSV `String` |
| `export_operations_input` | `operation_ids: &[i64]` | Formatted-update CSV `String` |
| `export_all_replayable_inputs` | none | Formatted-update CSV `String` |

Delete and custom SQL operations have audit rows but no formatted input.
Selected input export returns an error if any selected operation is not
replayable. Desktop audit and formatted-input export commands take an absolute
destination path selected by the caller and write the resulting CSV there with
the same configured delimiter.
