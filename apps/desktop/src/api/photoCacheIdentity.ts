export type PhotoCacheSource = {
  modified_at_ns: number;
  file_size: number;
};

export function photoCacheIdentity(
  photo: PhotoCacheSource,
  libraryUuid: string | null,
): string {
  return [libraryUuid || "unregistered", photo.modified_at_ns, photo.file_size].join(":");
}
