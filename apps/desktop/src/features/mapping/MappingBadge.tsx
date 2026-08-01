import type { PhotoTaxonStatus } from "../../api/mapping";

export function MappingBadge({ status }: { status: PhotoTaxonStatus }) {
  return <span className={`mapping-badge ${status}`}>{status}</span>;
}
