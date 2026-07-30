import { Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  displayTaxon,
  errorMessage,
  formatBytes,
  getPhotoMapping,
  getPhotoMetadata,
  getTaxonDetailNode,
  type Photo,
  type PhotoMetadata,
} from "./api";
import { Busy, PhotoStage } from "./components";
import { useViewState } from "./viewState";
import { usePhotoMutation } from "./photoMutations";

export function PhotoDetailView({ photo }: { photo: Photo }) {
  const [metadata, setMetadata] = useViewState<PhotoMetadata | null>("photo-detail.metadata", null);
  const [taxon, setTaxon] = useViewState("photo-detail.taxon", "");
  const [detailScrollTop, setDetailScrollTop] = useViewState("photo-detail.scroll-top", 0);
  const [error, setError] = useState("");
  const [copied, setCopied] = useState("");
  const [mappingRefresh, setMappingRefresh] = useState(0);
  const detailRef = useRef<HTMLDListElement>(null);
  usePhotoMutation((mutation) => {
    if (
      mutation.kind === "mapping"
      && (mutation.photoId === null || mutation.photoId === photo.photo_id)
    ) {
      setMappingRefresh((current) => current + 1);
    }
  });

  useEffect(() => {
    let active = true;
    Promise.all([
      getPhotoMetadata(photo.photo_id),
      getPhotoMapping(photo.photo_id).then(async (mapping) => {
        if (mapping.status !== "matched" || mapping.taxon_id === null) return "";
        return displayTaxon((await getTaxonDetailNode(mapping.taxon_id)).summary);
      }),
    ]).then(([nextMetadata, nextTaxon]) => {
      if (!active) return;
      setMetadata(nextMetadata);
      setTaxon(nextTaxon);
    }).catch((nextError) => setError(errorMessage(nextError)));
    return () => { active = false; };
  }, [photo, mappingRefresh]);

  useEffect(() => {
    if (detailRef.current) detailRef.current.scrollTop = detailScrollTop;
  }, [metadata]);

  function copy(label: string, value: string) {
    void navigator.clipboard.writeText(value).then(() => {
      setCopied(label);
      window.setTimeout(() => setCopied((current) => current === label ? "" : current), 1200);
    }).catch((nextError) => setError(errorMessage(nextError)));
  }

  const hasLocation = metadata?.longitude !== null
    && metadata?.longitude !== undefined
    && metadata.latitude !== null
    && metadata.latitude !== undefined;
  const hasDimensions = metadata?.width !== null
    && metadata?.width !== undefined
    && metadata.height !== null
    && metadata.height !== undefined;

  return (
    <div className="photo-detail-view">
      <header className="two-line-heading">
        <strong>{photo.filename}</strong>
        <span>{taxon || "No matched taxon"}</span>
      </header>
      <div className="photo-detail-content">
        <PhotoStage photo={photo} compact />
        {!metadata && !error ? <Busy label="Loading details" /> : (
          <dl
            className="detail-grid"
            ref={detailRef}
            onScroll={(event) => setDetailScrollTop(event.currentTarget.scrollTop)}
          >
            <DetailValue label="Path" value={photo.relative_path} copied={copied} onCopy={copy} />
            <DetailValue label="Size" value={formatBytes(photo.file_size)} copied={copied} onCopy={copy} />
            <DetailValue label="Captured" value={metadata?.captured_at ?? "-"} copied={copied} onCopy={copy} />
            <DetailValue label="Camera" value={metadata?.camera ?? "-"} copied={copied} onCopy={copy} />
            <DetailValue label="Dimensions" value={hasDimensions ? `${metadata.width} x ${metadata.height}` : "-"} copied={copied} onCopy={copy} />
            <DetailValue label="Location" value={hasLocation ? `${metadata.latitude}, ${metadata.longitude}` : "-"} copied={copied} onCopy={copy} />
            <DetailValue label="EXIF" value={metadata?.exif_json ?? "-"} copied={copied} onCopy={copy} multiline />
          </dl>
        )}
        {error && <div className="inline-error">{error}</div>}
      </div>
    </div>
  );
}

function DetailValue({
  label,
  value,
  copied,
  onCopy,
  multiline = false,
}: {
  label: string;
  value: string;
  copied: string;
  onCopy: (label: string, value: string) => void;
  multiline?: boolean;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className="detail-value">
        {multiline ? <pre>{value}</pre> : <span>{value}</span>}
        <button type="button" title={`Copy ${label}`} onClick={() => onCopy(label, value)}>
          <Copy size={13} />
          <span>{copied === label ? "Copied" : "Copy"}</span>
        </button>
      </dd>
    </>
  );
}
