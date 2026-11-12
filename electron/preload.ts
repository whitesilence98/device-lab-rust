import { contextBridge, ipcRenderer } from 'electron';
import type { DeviceLabAPI, Preferences } from './types';

const api: DeviceLabAPI = {
  scan: () => ipcRenderer.invoke('devices:scan'),
  appearance: () => ipcRenderer.invoke('appearance:load'),
  saveAppearance: (preferences: Preferences) => ipcRenderer.invoke('appearance:save', preferences),
};
contextBridge.exposeInMainWorld('deviceLab', api);
