import { useEffect, useState } from "react";
import {
  displayTaxon,
  errorMessage,
  formatBytes,
  getPhotoMetadata,
  getPhotoTaxonMatch,
  getTaxonDetailNode,
  type Photo,
  type PhotoMetadata,
} from "./api";
import { Busy, PhotoStage } from "./components";

export function PhotoDetailView({ photo }: { photo: Photo }) {
  const [metadata, setMetadata] = useState<PhotoMetadata | null>(null);
  const [taxon, setTaxon] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    Promise.all([
      getPhotoMetadata(photo.photo_id),
      getPhotoTaxonMatch(photo.photo_id).then(async (match) => {
        if (match.mapping.status !== "matched" || match.mapping.taxon_id === null) return "";
        return displayTaxon((await getTaxonDetailNode(match.mapping.taxon_id)).summary);
      }),
    ]).then(([nextMetadata, nextTaxon]) => {
      if (!active) return;
      setMetadata(nextMetadata);
      setTaxon(nextTaxon);
    }).catch((nextError) => setError(errorMessage(nextError)));
    return () => { active = false; };
  }, [photo]);

  return (
    <div className="photo-detail-view">
      <header className="two-line-heading">
        <strong>{photo.filename}</strong>
        <span>{taxon || "No matched taxon"}</span>
      </header>
      <div className="photo-detail-content">
        <PhotoStage photo={photo} compact />
        {!metadata && !error ? <Busy label="Loading details" /> : (
          <dl className="detail-grid">
            <dt>Path</dt><dd>{photo.relative_path}</dd>
            <dt>Size</dt><dd>{formatBytes(photo.file_size)}</dd>
            <dt>Captured</dt><dd>{metadata?.captured_at ?? "-"}</dd>
            <dt>Camera</dt><dd>{metadata?.camera ?? "-"}</dd>
            <dt>Dimensions</dt><dd>{metadata?.width && metadata.height ? `${metadata.width} x ${metadata.height}` : "-"}</dd>
            <dt>Location</dt><dd>{metadata?.longitude && metadata.latitude ? `${metadata.latitude}, ${metadata.longitude}` : "-"}</dd>
            <dt>EXIF</dt><dd><pre>{metadata?.exif_json ?? "-"}</pre></dd>
          </dl>
        )}
        {error && <div className="inline-error">{error}</div>}
      </div>
    </div>
  );
}
