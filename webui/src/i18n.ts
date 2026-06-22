export type UiLanguage = "zh-CN" | "en-US";

export const DEFAULT_UI_LANGUAGE: UiLanguage = "zh-CN";
export const UI_LANGUAGE_STORAGE_KEY = "logagent.webui.language";

export function normalizeUiLanguage(value: unknown): UiLanguage {
  return value === "en-US" ? "en-US" : "zh-CN";
}

export const languageOptions: Array<{ value: UiLanguage; label: string }> = [
  { value: "zh-CN", label: "简体中文" },
  { value: "en-US", label: "English" }
];

export const appCopy = {
  "zh-CN": {
    productName: "LogAgent 工具工作台",
    productSubtitle: "本地工具、运行记录、Artifact 与 MCP",
    serverHealthy: "Server 正常",
    serverUnavailable: "Server 不可用",
    checking: "检查中",
    apiKeyRequired: "需要 API Key",
    apiKeyPlaceholder: "API Key",
    navTools: "工具",
    navRuns: "运行记录",
    navMetadata: "Metadata",
    navFetch: "Fetch",
    navExecutors: "Executors",
    navMcp: "MCP",
    navCases: "Cases",
    navSystemContext: "系统上下文",
    navSettings: "设置",
    languageLabel: "语言"
  },
  "en-US": {
    productName: "LogAgent Tool Workbench",
    productSubtitle: "Local tools, runs, artifacts, and MCP",
    serverHealthy: "Server healthy",
    serverUnavailable: "Server unavailable",
    checking: "Checking",
    apiKeyRequired: "API Key required",
    apiKeyPlaceholder: "API Key",
    navTools: "Tools",
    navRuns: "Runs",
    navMetadata: "Metadata",
    navFetch: "Fetch",
    navExecutors: "Executors",
    navMcp: "MCP",
    navCases: "Cases",
    navSystemContext: "System Context",
    navSettings: "Settings",
    languageLabel: "Language"
  }
} as const;
