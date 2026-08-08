import { Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  getPhotoMetadata,
  type Photo,
  type PhotoMetadata,
} from "../../api/photos";
import { errorMessage, formatBytes } from "../../api/common";
import { getPhotoMapping } from "../../api/mapping";
import { displayTaxonDetail, getTaxonDetail } from "../../api/taxonomy";
import { Busy, Button } from "../../shared/ui";
import { PhotoStage } from "./PhotoMedia";
import { useViewState } from "../../shared/viewState";
import { usePhotoMutation } from "./photoMutations";
import { ResizablePanels } from "../../shared/ResizablePanels";

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
        return displayTaxonDetail(await getTaxonDetail(mapping.taxon_id));
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
      <ResizablePanels
        className="photo-detail-content"
        initialRatio={0.52}
        minFirst={320}
        minSecond={300}
        separatorLabel="Resize photo and details"
        stateKey="photo-detail.columns"
        first={<PhotoStage photo={photo} compact />}
        second={(<div className="photo-detail-sidebar">
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
        </div>)}
      />
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
        <Button size="small" title={`Copy ${label}`} onClick={() => onCopy(label, value)}>
          <Copy size={13} />
          <span>{copied === label ? "Copied" : "Copy"}</span>
        </Button>
      </dd>
    </>
  );
}
