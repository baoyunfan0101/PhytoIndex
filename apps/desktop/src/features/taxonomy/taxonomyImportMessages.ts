export function formatTaxonomyImportApplyMessage(warnings: string[] | null | undefined) {
  return [
    "Taxonomy database replaced successfully. Photo mappings are being rebuilt in the background.",
    ...(warnings ?? []),
  ].join(" ");
}
