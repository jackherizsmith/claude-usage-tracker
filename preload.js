const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('claude', {
  getUsage: () => ipcRenderer.invoke('get-usage'),
  refreshNow: () => ipcRenderer.invoke('refresh-now'),
  getAnalytics: (days) => ipcRenderer.invoke('get-analytics', days),
  setHeight: (h) => ipcRenderer.send('set-height', h),
  onRefresh: (cb) => ipcRenderer.on('refresh', cb),
  quit: () => ipcRenderer.send('quit')
})
