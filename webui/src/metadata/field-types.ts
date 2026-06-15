export const OPEN_GEMINI_FIELD_TYPE_LABELS = [
  "unknown",
  "int",
  "uint",
  "float",
  "string",
  "boolean",
  "tag",
  "last"
] as const;

export function openGeminiFieldTypeLabel(type?: number | null) {
  if (type == null) return "unknown";
  return OPEN_GEMINI_FIELD_TYPE_LABELS[type] ?? `type-${type}`;
}
