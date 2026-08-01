import { useEffect, useState } from "react";
import { getAppVersion } from "../api/updater";

export function NativeAboutOverlay({ onClose }: { onClose: () => void }) {
  const [version, setVersion] = useState("3.0.0");

  useEffect(() => {
    void getAppVersion().then(setVersion);
  }, []);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="native-about-overlay"
      role="presentation"
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="native-about-dialog" role="dialog" aria-label="About Vividarium" aria-modal="true">
        <h1>Vividarium</h1>
        <div><span>Version:</span><strong>{version}</strong></div>
        <div><span>Author:</span><strong>Yunfan Bao</strong></div>
        <div><span>GitHub:</span><a href="https://github.com/baoyunfan0101/Vividarium" target="_blank" rel="noreferrer">github.com/baoyunfan0101/Vividarium</a></div>
      </div>
    </div>
  );
}
