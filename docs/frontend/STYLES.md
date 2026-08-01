# Style Modules

Location: `apps/desktop/src/styles`

`index.css` is the only stylesheet imported by `main.tsx`. It composes the
theme, application shell, shared UI, and business-domain styles.

| File | Ownership |
| --- | --- |
| `theme.css` | Color variables, typography, reset, and focus behavior. |
| `app.css` | Desktop shell, activity bar, tabs, toolbar, native About, popovers, and status bar. |
| `shared.css` | Buttons, virtual collections, segmented controls, editor, modal, loading, and empty states. |
| `photos.css` | Photo search, empty workspace, browsers, media, folder tree, detail view, map, and photo context menu. |
| `mapping.css` | Mapping workspace, state badges, candidates, and editor layout. |
| `taxonomy.css` | Taxon cards, taxonomy pages, formatted input, SQL results, and base import. |
| `operations.css` | Operation summaries, audit rows, and history actions. |
| `settings.css` | Settings navigation, storage, libraries, taxonomy databases, naming, map, and hook-test layouts. |

Feature components use domain class names and place new rules in their owning
domain file. A rule belongs in `shared.css` only after the corresponding UI is
used by more than one feature domain.
