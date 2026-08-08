export const photoNamePriorityFields = [
  "species_sci",
  "species_zh",
  "genus_sci",
  "genus_zh",
  "family_sci",
  "family_zh",
] as const;

export const photoNamePriorityLabels: Record<typeof photoNamePriorityFields[number], string> = {
  species_sci: "Species scientific",
  species_zh: "Species Chinese",
  genus_sci: "Genus scientific",
  genus_zh: "Genus Chinese",
  family_sci: "Family scientific",
  family_zh: "Family Chinese",
};

export const photoFilenameFormatFields = [
  { field: "family_zh", label: "Family Chinese" },
  { field: "family_sci", label: "Family scientific" },
  { field: "genus_zh", label: "Genus Chinese" },
  { field: "genus_sci", label: "Genus scientific" },
  { field: "species_zh", label: "Species Chinese" },
  { field: "species_sci", label: "Species scientific" },
] as const;

export type PhotoFilenameFormatField = typeof photoFilenameFormatFields[number]["field"];
export type PhotoFilenameFormatValue = Record<PhotoFilenameFormatField, boolean>;

export function defaultPhotoFilenameFormatSettings(): PhotoFilenameFormatValue {
  return {
    family_zh: false,
    family_sci: false,
    genus_zh: false,
    genus_sci: false,
    species_zh: false,
    species_sci: true,
  };
}

export function normalizePhotoFilenameFormatSettings(
  value: Partial<Record<PhotoFilenameFormatField, unknown>>,
): PhotoFilenameFormatValue {
  const settings = defaultPhotoFilenameFormatSettings();
  for (const { field } of photoFilenameFormatFields) {
    if (typeof value[field] === "boolean") settings[field] = value[field];
  }
  return settings;
}

export function photoNamePriorityChanged(
  saved: readonly string[],
  current: readonly string[],
): boolean {
  return saved.length !== current.length || saved.some((field, index) => field !== current[index]);
}

export function photoFilenameFormatChanged(
  saved: PhotoFilenameFormatValue,
  current: PhotoFilenameFormatValue,
): boolean {
  return photoFilenameFormatFields.some(({ field }) => saved[field] !== current[field]);
}
