const STORAGE_KEY = "logagent.webui.tasks"
const API_KEY_STORAGE = "logagent.webui.apiKey"
const CHUNK_BYTES = 512 * 1024

const state = {
  activeTaskId: null,
  artifacts: null
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
  activeTaskLabel: document.querySelector("#activeTaskLabel")
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

document.querySelectorAll(".nav-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view))
})

els.apiKey.addEventListener("change", () => {
  localStorage.setItem(API_KEY_STORAGE, els.apiKey.value.trim())
})

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
  const file = els.fileInput.files[0]
  if (!file) {
    log("请选择文件")
    return
  }
  if (!apiKey()) {
    log("请填写 API Key")
    return
  }

  setProgress(0)
  log(`开始上传 ${file.name} (${formatBytes(file.size)})`)

  try {
    const upload = await uploadFile(file)
    log(`上传完成: ${upload.uploadId}`)
    const task = await createTask(upload.uploadId)
    log(`任务完成: ${task.taskId}`)
    saveTask({
      taskId: task.taskId,
      filename: upload.filename,
      size: upload.size,
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

async function uploadFile(file) {
  if (file.size <= CHUNK_BYTES) {
    const form = new FormData()
    form.append("filename", file.name)
    form.append("file", file, file.name)
    const res = await fetchJson("/api/uploads", {
      method: "POST",
      headers: authHeaders(),
      body: form
    })
    setProgress(100)
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
    setProgress(Math.round((offset / file.size) * 100))
  }

  return fetchJson(`/api/uploads/${encodeURIComponent(init.uploadId)}/complete`, {
    method: "POST",
    headers: authHeaders()
  })
}

async function createTask(uploadId) {
  const sourceUrl = els.sourceUrl.value.trim()
  return fetchJson("/api/tasks", {
    method: "POST",
    headers: jsonHeaders(),
    body: JSON.stringify({
      uploadId,
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
