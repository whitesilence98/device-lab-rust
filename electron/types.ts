export interface Device {
  instance_id: string;
  container_id: string | null;
  name: string;
  manufacturer: string;
  class: string;
  enumerator: string;
  parent: string;
  hardware_ids: string[];
  usb_ids: [number, number] | null;
}
export interface Group { key: string; members: Device[] }
export interface Snapshot {
  groups: Group[];
  recordCount: number;
  hasSnapshot: boolean;
  error: string | null;
  changes: { added: number; updated: number; removed: number };
}
export interface Preferences { light: boolean; scale: number }
export interface Appearance {
  preferences: Preferences;
  error: string | null;
}
export interface DeviceLabAPI {
  scan(): Promise<Snapshot>;
  appearance(): Promise<Appearance>;
  saveAppearance(preferences: Preferences): Promise<string>;
}
