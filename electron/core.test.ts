import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaults, emptySnapshot, encodePreferences, matches, parsePreferences, parseSnapshot, validPreferences } from './core';
import type { Group } from './types';

test('existing preferences round-trip; malformed files cannot be saved over', () => {
  assert.deepEqual(parsePreferences(encodePreferences(defaults)), defaults);
  assert.deepEqual(parsePreferences('device-lab-preferences-v1\r\ntheme=light\r\nscale=1.6\r\n'), { light: true, scale: 1.6 });
  for (const text of ['garbage', 'device-lab-preferences-v1\ntheme=dark\nscale=NaN', 'device-lab-preferences-v1\ntheme=light\nscale=99']) {
    assert.throws(() => parsePreferences(text));
  }
  for (const p of [null, {}, { light: 'yes', scale: 1 }, { light: true, scale: Infinity }, { light: false, scale: 0 }]) assert.equal(validPreferences(p), false);
});

const group: Group = { key: 'container:abc', members: [{
  instance_id: 'USB\\A', container_id: 'abc', name: 'Learning Mouse', manufacturer: 'Example',
  class: 'Mouse', enumerator: 'USB', parent: '', hardware_ids: ['USB\\VID_1234&PID_5678'], usb_ids: [0x1234, 0x5678],
}, {
  instance_id: 'HID\\A', container_id: 'abc', name: 'Keyboard interface', manufacturer: '',
  class: 'Keyboard', enumerator: 'HID', parent: '', hardware_ids: [], usb_ids: null,
}] };

test('protocol validates records and rejects invalid identities/counts', () => {
  const snapshot = { ...emptySnapshot, groups: [group], recordCount: 2, hasSnapshot: true };
  assert.deepEqual(parseSnapshot(JSON.stringify(snapshot)), snapshot);
  assert.throws(() => parseSnapshot(JSON.stringify({ ...snapshot, recordCount: 1 })));
  assert.throws(() => parseSnapshot(JSON.stringify({ ...snapshot, groups: [{ key: 'x', members: [] }] })));
  assert.throws(() => parseSnapshot('null'));
  assert.throws(() => parseSnapshot(JSON.stringify({ ...snapshot, changes: { added: -1, updated: 0, removed: 0 } })));
});

test('search checks each record; class and query must match the same record', () => {
  assert.equal(matches(group, 'MOUSE', 'Mouse'), true);
  assert.equal(matches(group, 'vid_1234', ''), true);
  assert.equal(matches(group, 'mouse', 'Keyboard'), false);
  assert.equal(matches(group, 'keyboard', 'Keyboard'), true);
  assert.equal(matches(group, 'headset', ''), false);
});
