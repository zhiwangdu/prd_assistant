const STORAGE_KEY = "logagent.webui.tasks"
const API_KEY_STORAGE = "logagent.webui.apiKey"
const CHUNK_BYTES = 512 * 1024

const state = {
  activeTaskId: null,
  artifacts: null,
  metadataImportId: null
}

const els = {
  apiKey: document.querySelector("#apiKey"),
  sourceUrl: document.querySelector("#sourceUrl"),
  fileInput: document.querySelector("#fileInput"),
  progressBar: document.querySelector("#progressBar"),
  statusLog: document.querySelector("#statusLog"),
  healthDot: document.querySelector("#healthDot"),
  healthText: document.querySelector("#healthText"),
  taskList: document.querySelector("#taskList"),
  manifestFiles: document.querySelector("#manifestFiles"),
  grepMatches: document.querySelector("#grepMatches"),
  rawArtifacts: document.querySelector("#rawArtifacts"),
  activeTaskLabel: document.querySelector("#activeTaskLabel"),
  metadataInstanceId: document.querySelector("#metadataInstanceId"),
  metadataClusterId: document.querySelector("#metadataClusterId"),
  metadataResult: document.querySelector("#metadataResult"),
  metadataTemplateType: document.querySelector("#metadataTemplateType"),
  metadataFilename: document.querySelector("#metadataFilename"),
  metadataFetchUrl: document.querySelector("#metadataFetchUrl"),
  metadataTemplate: document.querySelector("#metadataTemplate"),
  metadataImportResult: document.querySelector("#metadataImportResult")
}

els.apiKey.value = localStorage.getItem(API_KEY_STORAGE) || ""

document.querySelector("#checkHealth").addEventListener("click", checkHealth)
document.querySelector("#uploadAndRun").addEventListener("click", uploadAndRun)
document.querySelector("#loadArtifacts").addEventListener("click", () => {
  if (state.activeTaskId) {
    loadArtifacts(state.activeTaskId)
  } else {
    log("请选择一个任务")
  }
})
document.querySelector("#clearTasks").addEventListener("click", () => {
  localStorage.setItem(STORAGE_KEY, "[]")
  renderTasks()
})
document.querySelector("#queryInstance").addEventListener("click", queryInstance)
document.querySelector("#queryCluster").addEventListener("click", queryCluster)
document.querySelector("#fetchMetadataImport").addEventListener("click", fetchMetadataImport)
document.querySelector("#previewMetadataImport").addEventListener("click", previewMetadataImport)
document.querySelector("#confirmMetadataImport").addEventListener("click", confirmMetadataImport)

document.querySelectorAll(".nav-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view))
})

els.apiKey.addEventListener("change", () => {
  localStorage.setItem(API_KEY_STORAGE, els.apiKey.value.trim())
})

els.metadataTemplate.value = `{
  "ClusterID": 6735497445922383781,
  "MetaNodes": [
    {
      "ID": 1,
      "Host": "127.0.0.1:8091",
      "RPCAddr": "127.0.0.1:8092",
      "TCPHost": "127.0.0.1:8088",
      "Status": 0
    }
  ],
  "DataNodes": [
    {
      "ID": 2,
      "Host": "127.0.0.1:8400",
      "TCPHost": "127.0.0.1:8401",
      "Status": 1,
      "Az": ""
    }
  ],
  "SqlNodes": [
    {
      "ID": 3,
      "TCPHost": ":8086",
      "Status": 1
    }
  ],
  "Databases": {
    "mydb": { "Name": "mydb" }
  }
}`

renderTasks()
checkHealth()

async function checkHealth() {
  try {
    const res = await fetch("/health")
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`)
    }
    const body = await res.json()
    els.healthDot.className = "dot"
    els.healthText.textContent = body.status || "ok"
  } catch (err) {
    els.healthDot.className = "dot error"
    els.healthText.textContent = err.message
  }
}

async function uploadAndRun() {
  const files = Array.from(els.fileInput.files)
  if (!files.length) {
    log("请选择文件")
    return
  }
  if (!apiKey()) {
    log("请填写 API Key")
    return
  }

  setProgress(0)
  const totalSize = files.reduce((sum, file) => sum + file.size, 0)
  const progressTotal = Math.max(totalSize, 1)
  let uploadedSize = 0
  log(`开始上传 ${files.length} 个文件 (${formatBytes(totalSize)})`)

  try {
    const uploads = []
    for (const file of files) {
      log(`上传 ${file.name} (${formatBytes(file.size)})`)
      const upload = await uploadFile(file, (fileProgress) => {
        const current = uploadedSize + Math.round(file.size * fileProgress)
        setProgress(Math.round((current / progressTotal) * 100))
      })
      uploadedSize += file.size
      setProgress(Math.round((uploadedSize / progressTotal) * 100))
      uploads.push(upload)
      log(`上传完成: ${upload.uploadId}`)
    }
    setProgress(100)
    const task = await createTask(uploads.map((upload) => upload.uploadId))
    log(`任务完成: ${task.taskId}`)
    saveTask({
      taskId: task.taskId,
      filename: uploads.map((upload) => upload.filename).join(", "),
      size: uploads.reduce((sum, upload) => sum + upload.size, 0),
      createdAt: new Date().toISOString()
    })
    state.activeTaskId = task.taskId
    renderTasks()
    await loadArtifacts(task.taskId)
    showView("evidence")
  } catch (err) {
    log(`失败: ${err.message}`)
  }
}

async function uploadFile(file, onProgress = () => {}) {
  if (file.size <= CHUNK_BYTES) {
    const form = new FormData()
    form.append("filename", file.name)
    form.append("file", file, file.name)
    const res = await fetchJson("/api/uploads", {
      method: "POST",
      headers: authHeaders(),
      body: form
    })
    onProgress(1)
    return res
  }

  const init = await fetchJson("/api/uploads/init", {
    method: "POST",
    headers: jsonHeaders(),
    body: JSON.stringify({ filename: file.name, size: file.size })
  })

  let offset = 0
  while (offset < file.size) {
    const next = Math.min(offset + CHUNK_BYTES, file.size)
    const chunk = file.slice(offset, next)
    await fetchJson(`/api/uploads/${encodeURIComponent(init.uploadId)}/chunks?offset=${offset}`, {
      method: "POST",
      headers: authHeaders(),
      body: chunk
    })
    offset = next
    onProgress(offset / file.size)
  }

  return fetchJson(`/api/uploads/${encodeURIComponent(init.uploadId)}/complete`, {
    method: "POST",
    headers: authHeaders()
  })
}

async function createTask(uploadIds) {
  const sourceUrl = els.sourceUrl.value.trim()
  return fetchJson("/api/tasks", {
    method: "POST",
    headers: jsonHeaders(),
    body: JSON.stringify({
      uploadIds,
      sourceUrl: sourceUrl || null
    })
  })
}

async function loadArtifacts(taskId) {
  const artifacts = await fetchJson(`/api/tasks/${encodeURIComponent(taskId)}/artifacts`, {
    headers: authHeaders()
  })
  state.activeTaskId = taskId
  state.artifacts = artifacts
  els.activeTaskLabel.textContent = taskId
  renderArtifacts(artifacts)
  showView("evidence")
}

async function queryInstance() {
  const instanceId = els.metadataInstanceId.value.trim()
  if (!instanceId) {
    renderMetadataResult("请输入 Instance ID")
    return
  }
  try {
    const body = await fetchJson(`/api/metadata/instances/${encodeURIComponent(instanceId)}`, {
      headers: authHeaders()
    })
    renderMetadataResult(JSON.stringify(body, null, 2), true)
  } catch (err) {
    renderMetadataResult(`查询失败: ${err.message}`)
  }
}

async function queryCluster() {
  const clusterId = els.metadataClusterId.value.trim()
  if (!clusterId) {
    renderMetadataResult("请输入 Cluster ID")
    return
  }
  try {
    const cluster = await fetchJson(`/api/metadata/clusters/${encodeURIComponent(clusterId)}`, {
      headers: authHeaders()
    })
    const nodes = await fetchJson(`/api/metadata/clusters/${encodeURIComponent(clusterId)}/nodes`, {
      headers: authHeaders()
    })
    renderMetadataResult(JSON.stringify({ ...cluster, nodes: nodes.nodes }, null, 2), true)
  } catch (err) {
    renderMetadataResult(`查询失败: ${err.message}`)
  }
}

async function previewMetadataImport() {
  if (!apiKey()) {
    renderMetadataImport("请填写 API Key")
    return
  }
  try {
    const preview = await fetchJson("/api/metadata/imports", {
      method: "POST",
      headers: jsonHeaders(),
      body: JSON.stringify({
        templateType: els.metadataTemplateType.value.trim(),
        filename: els.metadataFilename.value.trim() || null,
        content: els.metadataTemplate.value
      })
    })
    state.metadataImportId = preview.importId
    renderMetadataImport(renderMetadataPreview(preview), true)
  } catch (err) {
    renderMetadataImport(`预览失败: ${err.message}`)
  }
}

async function fetchMetadataImport() {
  if (!apiKey()) {
    renderMetadataImport("请填写 API Key")
    return
  }
  const url = els.metadataFetchUrl.value.trim()
  if (!url) {
    renderMetadataImport("请输入真实元数据 URL")
    return
  }
  try {
    const preview = await fetchJson("/api/metadata/imports/fetch", {
      method: "POST",
      headers: jsonHeaders(),
      body: JSON.stringify({
        url,
        templateType: els.metadataTemplateType.value.trim() || "opengemini",
        filename: els.metadataFilename.value.trim() || url
      })
    })
    state.metadataImportId = preview.importId
    renderMetadataImport(renderMetadataPreview(preview), true)
  } catch (err) {
    renderMetadataImport(`拉取失败: ${err.message}`)
  }
}

async function confirmMetadataImport() {
  if (!state.metadataImportId) {
    renderMetadataImport("请先预览导入")
    return
  }
  try {
    const response = await fetchJson(
      `/api/metadata/imports/${encodeURIComponent(state.metadataImportId)}/confirm`,
      {
        method: "POST",
        headers: authHeaders()
      }
    )
    renderMetadataImport(JSON.stringify(response, null, 2), true)
  } catch (err) {
    renderMetadataImport(`确认失败: ${err.message}`)
  }
}

async function fetchJson(url, options = {}) {
  const res = await fetch(url, options)
  const text = await res.text()
  const body = text ? JSON.parse(text) : {}
  if (!res.ok) {
    throw new Error(body.error || `HTTP ${res.status}`)
  }
  return body
}

function authHeaders() {
  return {
    Authorization: `Bearer ${apiKey()}`
  }
}

function jsonHeaders() {
  return {
    ...authHeaders(),
    "Content-Type": "application/json"
  }
}

function apiKey() {
  return els.apiKey.value.trim()
}

function renderArtifacts(artifacts) {
  const files = artifacts.manifest?.files || []
  els.manifestFiles.className = files.length ? "table-like" : "table-like empty"
  els.manifestFiles.innerHTML = files.length
    ? files.map((file) => `
        <div class="data-row">
          <code>${escapeHtml(file.path)}</code>
          <small>${formatBytes(file.size)}</small>
        </div>
      `).join("")
    : "暂无数据"

  const matches = artifacts.grepResults?.matches || []
  els.grepMatches.className = matches.length ? "table-like" : "table-like empty"
  els.grepMatches.innerHTML = matches.length
    ? matches.map((match) => `
        <div class="data-row">
          <code>${escapeHtml(match.file)}:${match.line}</code>
          <small>${escapeHtml(match.keyword)}</small>
          <div>${escapeHtml(match.text)}</div>
        </div>
      `).join("")
    : "暂无命中"

  els.rawArtifacts.textContent = JSON.stringify(artifacts, null, 2)
}

function renderTasks() {
  const tasks = getTasks()
  els.taskList.innerHTML = tasks.length
    ? tasks.map((task) => `
        <div class="task-row">
          <div>
            <strong>${escapeHtml(task.taskId)}</strong>
            <span>${escapeHtml(task.filename)}</span>
          </div>
          <span>${formatBytes(task.size)} · ${new Date(task.createdAt).toLocaleString()}</span>
          <button class="button secondary" type="button" data-task-id="${escapeHtml(task.taskId)}">查看</button>
        </div>
      `).join("")
    : `<div class="table-like empty">暂无任务</div>`

  els.taskList.querySelectorAll("[data-task-id]").forEach((button) => {
    button.addEventListener("click", () => loadArtifacts(button.dataset.taskId))
  })
}

function renderMetadataPreview(preview) {
  const lines = [
    `importId: ${preview.importId}`,
    `templateType: ${preview.templateType}`,
    `instances: ${preview.summary.instances}`,
    `clusters: ${preview.summary.clusters}`,
    `nodes: ${preview.summary.nodes}`,
    `warnings: ${preview.summary.warnings}`,
    `errors: ${preview.summary.errors}`,
    "",
    "changes:"
  ]
  for (const change of preview.changes || []) {
    lines.push(`- ${change.kind} ${change.id} ${change.action}: ${change.message}`)
  }
  if (preview.warnings?.length) {
    lines.push("", "warnings:")
    for (const warning of preview.warnings) {
      lines.push(`- ${warning}`)
    }
  }
  if (preview.errors?.length) {
    lines.push("", "errors:")
    for (const error of preview.errors) {
      lines.push(`- ${error}`)
    }
  }
  lines.push("", "raw:", JSON.stringify(preview, null, 2))
  return lines.join("\n")
}

function renderMetadataResult(value, preformatted = false) {
  els.metadataResult.className = "table-like"
  els.metadataResult.innerHTML = preformatted
    ? `<pre>${escapeHtml(value)}</pre>`
    : `<div class="data-row">${escapeHtml(value)}</div>`
}

function renderMetadataImport(value, preformatted = false) {
  els.metadataImportResult.className = "table-like"
  els.metadataImportResult.innerHTML = preformatted
    ? `<pre>${escapeHtml(value)}</pre>`
    : `<div class="data-row">${escapeHtml(value)}</div>`
}

function saveTask(task) {
  const tasks = getTasks().filter((item) => item.taskId !== task.taskId)
  tasks.unshift(task)
  localStorage.setItem(STORAGE_KEY, JSON.stringify(tasks.slice(0, 50)))
}

function getTasks() {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) || "[]")
  } catch {
    return []
  }
}

function showView(view) {
  document.querySelectorAll(".nav-item").forEach((button) => {
    button.classList.toggle("active", button.dataset.view === view)
  })
  document.querySelectorAll("[data-panel]").forEach((panel) => {
    panel.classList.toggle("hidden", panel.dataset.panel !== view)
  })
}

function setProgress(value) {
  els.progressBar.style.width = `${value}%`
}

function log(message) {
  const now = new Date().toLocaleTimeString()
  els.statusLog.textContent = `[${now}] ${message}\n${els.statusLog.textContent}`
}

function formatBytes(value) {
  if (!Number.isFinite(value)) {
    return "-"
  }
  const units = ["B", "KB", "MB", "GB"]
  let size = value
  let index = 0
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024
    index += 1
  }
  return `${size.toFixed(index === 0 ? 0 : 1)} ${units[index]}`
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;")
}
