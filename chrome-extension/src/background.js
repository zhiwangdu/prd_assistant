const DEFAULT_OPTIONS = {
  agentBaseUrl: "http://127.0.0.1:17321",
  urlPrefixes: [
    "https://xxx/download/",
    "https://logs.xxx.com/export/"
  ],
  fileSuffixes: [
    ".log",
    ".txt",
    ".zip",
    ".tar.gz",
    ".tgz",
    ".tar"
  ]
}

const pendingDownloads = new Map()

chrome.runtime.onInstalled.addListener(async () => {
  const existing = await chrome.storage.sync.get(DEFAULT_OPTIONS)
  await chrome.storage.sync.set({ ...DEFAULT_OPTIONS, ...existing })
})

chrome.downloads.onChanged.addListener((delta) => {
  if (delta.state?.current !== "complete") {
    return
  }

  chrome.downloads.search({ id: delta.id }, async (items) => {
    const item = items[0]
    if (!item) {
      return
    }

    const options = await getOptions()
    if (!isLogDownload(item, options)) {
      return
    }

    await askToImport(item)
  })
})

chrome.notifications.onButtonClicked.addListener(async (notificationId, buttonIndex) => {
  if (buttonIndex !== 0 || !pendingDownloads.has(notificationId)) {
    pendingDownloads.delete(notificationId)
    await chrome.notifications.clear(notificationId)
    return
  }

  const item = pendingDownloads.get(notificationId)
  pendingDownloads.delete(notificationId)
  await chrome.notifications.clear(notificationId)

  try {
    const result = await importDownload(item)
    await notify("LogAgent task created", result.url || `Task ${result.taskId} created`)
  } catch (error) {
    await notify("LogAgent import failed", error.message)
  }
})

chrome.notifications.onClosed.addListener((notificationId) => {
  pendingDownloads.delete(notificationId)
})

async function getOptions() {
  return chrome.storage.sync.get(DEFAULT_OPTIONS)
}

function isLogDownload(item, options) {
  const filename = (item.filename || item.finalUrl || item.url || "").toLowerCase()
  const url = item.finalUrl || item.url || ""

  const suffixMatched = options.fileSuffixes.some((suffix) =>
    filename.endsWith(suffix.toLowerCase())
  )
  const prefixMatched = options.urlPrefixes.some((prefix) =>
    prefix && url.startsWith(prefix)
  )

  return suffixMatched || prefixMatched
}

async function askToImport(item) {
  const notificationId = `logagent-import-${item.id}-${Date.now()}`
  pendingDownloads.set(notificationId, item)

  await chrome.notifications.create(notificationId, {
    type: "basic",
    iconUrl: "icons/icon.svg",
    title: "Send log to LogAgent?",
    message: item.filename || item.url || "Completed download",
    buttons: [
      { title: "Send to LogAgent" },
      { title: "Ignore" }
    ],
    priority: 2,
    requireInteraction: true
  })
}

async function importDownload(item) {
  const options = await getOptions()
  const response = await fetch(`${trimTrailingSlash(options.agentBaseUrl)}/imports`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      filePath: item.filename,
      filename: basename(item.filename),
      sourceUrl: item.finalUrl || item.url || null
    })
  })

  const bodyText = await response.text()
  const body = bodyText ? JSON.parse(bodyText) : {}
  if (!response.ok) {
    throw new Error(body.error || `Native Agent returned HTTP ${response.status}`)
  }
  return body
}

async function notify(title, message) {
  await chrome.notifications.create({
    type: "basic",
    iconUrl: "icons/icon.svg",
    title,
    message: String(message).slice(0, 220),
    priority: 1
  })
}

function basename(path) {
  const normalized = path || ""
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"))
  return index >= 0 ? normalized.slice(index + 1) : normalized
}

function trimTrailingSlash(value) {
  return value.replace(/\/+$/, "")
}
