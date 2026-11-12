import type { Device, Group, Preferences, Snapshot } from './types';

export const defaults: Preferences = { light: false, scale: 1 };
export const emptySnapshot: Snapshot = {
  groups: [], recordCount: 0, hasSnapshot: false, error: null,
  changes: { added: 0, updated: 0, removed: 0 },
};

export function validPreferences(value: unknown): value is Preferences {
  if (!value || typeof value !== 'object') return false;
  const p = value as Preferences;
  return typeof p.light === 'boolean' && typeof p.scale === 'number'
    && Number.isFinite(p.scale) && p.scale >= 0.8 && p.scale <= 1.6;
}
export function parsePreferences(text: string): Preferences {
  const match = /^device-lab-preferences-v1\r?\ntheme=(dark|light)\r?\nscale=([^\r\n]+)\r?\n?$/.exec(text);
  const preferences = { light: match?.[1] === 'light', scale: Number(match?.[2]) };
  if (!match || !match[2].trim() || !validPreferences(preferences)) {
    throw new Error('Invalid preferences; original file preserved. Saving disabled.');
  }
  return preferences;
}
export function encodePreferences(p: Preferences): string {
  if (!validPreferences(p)) throw new Error('Invalid appearance settings.');
  return `device-lab-preferences-v1\ntheme=${p.light ? 'light' : 'dark'}\nscale=${p.scale}\n`;
}

function validDevice(value: unknown): value is Device {
  if (!value || typeof value !== 'object') return false;
  const d = value as Device;
  return ['instance_id', 'name', 'manufacturer', 'class', 'enumerator', 'parent']
    .every(key => typeof d[key as keyof Device] === 'string')
    && (d.container_id === null || typeof d.container_id === 'string')
    && Array.isArray(d.hardware_ids) && d.hardware_ids.every(id => typeof id === 'string')
    && (d.usb_ids === null || (Array.isArray(d.usb_ids) && d.usb_ids.length === 2
      && d.usb_ids.every(id => Number.isInteger(id) && id >= 0 && id <= 65535)));
}
export function parseSnapshot(text: string): Snapshot {
  const s = JSON.parse(text) as Snapshot;
  const count = (n: unknown) => Number.isSafeInteger(n) && (n as number) >= 0;
  if (!s || !Array.isArray(s.groups) || !count(s.recordCount)
    || typeof s.hasSnapshot !== 'boolean' || !(s.error === null || typeof s.error === 'string')
    || !s.changes || ![s.changes.added, s.changes.updated, s.changes.removed].every(count)
    || !s.groups.every(g => g && typeof g.key === 'string' && Array.isArray(g.members)
      && g.members.length > 0 && g.members.every(validDevice))
    || s.groups.reduce((total, g) => total + g.members.length, 0) !== s.recordCount) {
    throw new Error('Invalid device helper response.');
  }
  return s;
}

export function matches(group: Group, query: string, deviceClass: string): boolean {
  const needle = query.toLowerCase();
  return group.members.some(d => (!deviceClass || d.class === deviceClass)
    && [d.name, d.manufacturer, d.class, d.instance_id, d.enumerator, ...d.hardware_ids]
      .some(value => value.toLowerCase().includes(needle)));
}
