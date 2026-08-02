import defaultPhotoFilenameHook from "../../../../crates/vividarium-core/src/naming/templates/photo_filename.rhai?raw";
import defaultSynonymAuthorityHook from "../../../../crates/vividarium-core/src/naming/templates/synonym_authority.rhai?raw";
import { call } from "./client";

export type NamingHookKind = "photo_filename" | "synonym_authority";
export type NamingHookSettings = { photo_filename: string | null; synonym_authority: string | null };
export type NamingHookTemplates = { photo_filename: string; synonym_authority: string };
export type NamingHookTestResult =
  | { kind: "photo_filename"; output: unknown }
  | { kind: "synonym_authority"; output: unknown };
export type NamingHookTestCase = { input: string; expected: NamingHookTestResult };
export type NamingHookTestCases = {
  photo_filename: NamingHookTestCase[];
  synonym_authority: NamingHookTestCase[];
};
export type NamingHookTestReport = {
  kind: NamingHookKind;
  passed: number;
  failed: number;
  cases: Array<NamingHookTestCase & {
    actual: NamingHookTestResult | null;
    passed: boolean;
    error: string | null;
  }>;
};
export type PhotoNameField =
  | "species_sci" | "species_zh" | "genus_sci" | "genus_zh" | "family_sci" | "family_zh";
export type PhotoNameMatchSettings = { priority: PhotoNameField[] };
export type PhotoFilenameFormatSettings = {
  family_zh: boolean;
  family_sci: boolean;
  genus_zh: boolean;
  genus_sci: boolean;
  species_zh: boolean;
  species_sci: boolean;
};

const defaultCases = (): NamingHookTestCases => ({
  photo_filename: [{
    input: "Herbertus dicranus010.jpg",
    expected: {
      kind: "photo_filename",
      output: {
        info: {
          family_sci: null,
          genus_sci: "Herbertus",
          species_sci: "Herbertus dicranus",
          family_zh: null,
          genus_zh: null,
          species_zh: null,
        },
        suffix: "010.jpg",
      },
    },
  }],
  synonym_authority: [{
    input: "Canis lupus (Linnaeus, 1758)",
    expected: {
      kind: "synonym_authority",
      output: { name: "Canis lupus", authority_year: "(Linnaeus, 1758)" },
    },
  }],
});

export const getNamingHookSettings = () =>
  call<NamingHookSettings>("get_naming_hook_settings", undefined, () => ({ photo_filename: null, synonym_authority: null }));
export const getNamingHookTemplates = () =>
  call<NamingHookTemplates>("get_naming_hook_templates", undefined, () => ({
    photo_filename: defaultPhotoFilenameHook,
    synonym_authority: defaultSynonymAuthorityHook,
  }));
export const getNamingHookTestCases = () =>
  call<NamingHookTestCases>("get_naming_hook_test_cases", undefined, defaultCases);
export const runNamingHookTests = (kind: NamingHookKind, script: string, cases: NamingHookTestCase[]) =>
  call<NamingHookTestReport>("run_naming_hook_tests", { kind, script, cases }, () => {
    return { kind, passed: cases.length, failed: 0, cases: cases.map((item) => ({
      ...item, actual: item.expected, passed: true, error: null,
    })) };
  });
export const saveNamingHook = (kind: NamingHookKind, script: string, cases: NamingHookTestCase[]) =>
  call<void>("save_naming_hook", { kind, script, cases }, () => undefined);
export const getPhotoNameMatchSettings = () =>
  call<PhotoNameMatchSettings>("get_photo_name_match_settings", undefined, () => ({
    priority: ["species_sci", "species_zh", "genus_sci", "genus_zh", "family_sci", "family_zh"],
  }));
export const setPhotoNameMatchSettings = (settings: PhotoNameMatchSettings) =>
  call<void>("set_photo_name_match_settings", { settings }, () => undefined);
export const getPhotoFilenameFormatSettings = () =>
  call<PhotoFilenameFormatSettings>("get_photo_filename_format_settings", undefined, () => ({
    family_zh: false,
    family_sci: false,
    genus_zh: false,
    genus_sci: false,
    species_zh: false,
    species_sci: true,
  }));
export const setPhotoFilenameFormatSettings = (settings: PhotoFilenameFormatSettings) =>
  call<void>("set_photo_filename_format_settings", { settings }, () => undefined);
export const getTaxonomyNameSeparator = () =>
  call<string>("get_taxonomy_name_separator", undefined, () => ";");
export const setTaxonomyNameSeparator = (separator: string) =>
  call<void>("set_taxonomy_name_separator", { separator }, () => undefined);
