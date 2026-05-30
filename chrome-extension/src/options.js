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
    ".tgz"
  ]
}

const agentBaseUrl = document.querySelector("#agentBaseUrl")
const urlPrefixes = document.querySelector("#urlPrefixes")
const fileSuffixes = document.querySelector("#fileSuffixes")
const save = document.querySelector("#save")
const status = document.querySelector("#status")

init()

async function init() {
  const options = await chrome.storage.sync.get(DEFAULT_OPTIONS)
  agentBaseUrl.value = options.agentBaseUrl
  urlPrefixes.value = options.urlPrefixes.join("\n")
  fileSuffixes.value = options.fileSuffixes.join("\n")
}

save.addEventListener("click", async () => {
  await chrome.storage.sync.set({
    agentBaseUrl: agentBaseUrl.value.trim() || DEFAULT_OPTIONS.agentBaseUrl,
    urlPrefixes: lines(urlPrefixes.value),
    fileSuffixes: lines(fileSuffixes.value)
  })
  status.textContent = "Saved"
  setTimeout(() => {
    status.textContent = ""
  }, 1500)
})

function lines(value) {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
}

