export const OPEN_GEMINI_FIELD_TYPE_LABELS = [
  "Unknown",
  "Integer",
  "Unsigned",
  "Float",
  "String",
  "Boolean",
  "Tag",
  "Unknown"
] as const;

type FieldTypeSource =
  | number
  | string
  | null
  | undefined
  | {
      typ?: number | string | null;
      type?: number | string | null;
      Typ?: number | string | null;
      Type?: number | string | null;
    };

export function openGeminiFieldTypeCode(source: FieldTypeSource) {
  const raw = typeof source === "object" && source !== null
    ? source.typ ?? source.type ?? source.Typ ?? source.Type
    : source;
  if (typeof raw === "number" && Number.isFinite(raw)) return raw;
  if (typeof raw === "string" && raw.trim()) {
    const parsed = Number(raw.trim());
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

export function openGeminiFieldTypeLabel(source: FieldTypeSource) {
  const type = openGeminiFieldTypeCode(source);
  if (type == null) return "Unknown";
  return OPEN_GEMINI_FIELD_TYPE_LABELS[type] ?? `Type ${type}`;
}
