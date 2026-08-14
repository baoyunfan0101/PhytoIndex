import { createContext, useContext, type ReactNode } from "react";

const PhotoLibraryIdentityContext = createContext<string | null>(null);

export function PhotoLibraryIdentityProvider({
  children,
  libraryUuid,
}: {
  children: ReactNode;
  libraryUuid: string | null;
}) {
  return (
    <PhotoLibraryIdentityContext.Provider value={libraryUuid}>
      {children}
    </PhotoLibraryIdentityContext.Provider>
  );
}

export function usePhotoLibraryIdentity(): string | null {
  return useContext(PhotoLibraryIdentityContext);
}
