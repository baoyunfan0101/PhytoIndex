import { searchPhotos } from "../../api/photos";
import { listTaxonPhotos } from "../../api/taxonomy";
import { useCursorPage } from "../../shared/useCursorPage";
import { PhotoBrowser } from "./PhotoBrowser";
import type { PhotoOpenHandlers } from "./PhotoInteraction";

export function PhotoSet({
  query,
  taxonId,
  refreshKey,
  handlers,
}: {
  query?: string;
  taxonId?: number;
  refreshKey?: number;
  handlers: PhotoOpenHandlers;
}) {
  const params = query !== undefined
    ? { kind: "search" as const, query }
    : { kind: "taxon" as const, taxonId: taxonId! };
  const page = useCursorPage({
    params,
    resetKey: query !== undefined ? `search:${query}:${refreshKey ?? 0}` : `taxon:${taxonId}`,
    stateKey: "photo-set.page",
    loadPage: (next, cursor) => next.kind === "search"
      ? searchPhotos(next.query, cursor)
      : listTaxonPhotos(next.taxonId, cursor),
  });

  return (
    <PhotoBrowser
      title={query !== undefined ? `Search: ${query}` : `Taxon ${taxonId}`}
      detail={page.loading ? "Loading" : page.error || undefined}
      loadingLabel={query !== undefined ? "Searching photos..." : "Loading photos..."}
      page={page}
      handlers={handlers}
    />
  );
}
