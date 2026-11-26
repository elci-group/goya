import { ipcMain } from "electron"

const channels = { read: "settings:read", write: "settings:write" }
const ns = "settings"

export function registerComputedChannels() {
  ipcMain.handle(channels.read, () => true)
  ipcMain.handle(channels.write, (_event, payload) => payload)
  ipcMain.handle(`${ns}:reset`, () => null)
  ipcMain.on(buildDynamicTopic(), (event) => {
    event.returnValue = null
  })
}
