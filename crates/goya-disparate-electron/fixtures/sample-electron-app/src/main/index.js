const { app, BrowserWindow, ipcMain, dialog } = require("electron")
const fs = require("fs")
const childProcess = require("child_process")

function createWindow() {
  const win = new BrowserWindow({ width: 800, height: 600 })
  win.loadFile("src/renderer/index.html")
}

ipcMain.handle("settings:read", () => {
  return JSON.parse(fs.readFileSync("settings.json", "utf8"))
})

ipcMain.on("sync-message", (event) => {
  dialog.showErrorBox("sync", event.returnValue)
})

const dynamicModules = { util: "./util.js" }
const loaded = require(dynamicModules.util)

const externalSpec = loadExternalSpec()
const lateModule = require(externalSpec)

const procAlias = childProcess

function runAlias() {
  procAlias.exec("echo hi")
}

if (process.env.DEBUG) {
  eval(process.env.DEBUG_EXPRESSION)
}

app.whenReady().then(createWindow)
