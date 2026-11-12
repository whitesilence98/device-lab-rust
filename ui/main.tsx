import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { defaults, emptySnapshot, matches } from '../electron/core';
import type { Device, DeviceLabAPI, Group, Preferences, Snapshot } from '../electron/types';
import './style.css';

declare global { interface Window { deviceLab?: DeviceLabAPI } }

function DeviceIcon({ kind }: { kind: string }) {
  const name = kind.toLowerCase();
  return <svg viewBox="0 0 160 110" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
    {name.includes('mouse') ? <><rect x="56" y="12" width="48" height="80" rx="24" /><path d="M80 12v33m-24 0h48M80 22v11" /></>
      : name.includes('keyboard') ? <><rect x="12" y="30" width="136" height="58" rx="7" />{Array.from({ length: 30 }, (_, i) => <rect key={i} x={23 + (i % 10) * 12} y={41 + Math.floor(i / 10) * 12} width="6" height="5" rx="1" />)}<path d="M55 79h50" /></>
      : name.includes('audio') || name.includes('media') ? <><path d="M38 64V51a42 42 0 0 1 84 0v13" /><rect x="30" y="53" width="20" height="36" rx="8" /><rect x="110" y="53" width="20" height="36" rx="8" /></>
      : name.includes('bluetooth') ? <path d="m66 22 38 33-38 33V22Zm0 66 38-33-38-33M46 34l58 43M46 77l58-43" />
      : <><rect x="31" y="18" width="98" height="64" rx="7" /><path d="M80 82v15m-25 0h50M43 69h74" /><circle cx="80" cy="44" r="8" /></>}
  </svg>;
}
const show = (value: string | null) => value || 'Unknown';

function Details({ group, stale, close }: { group: Group | undefined; stale: boolean; close(): void }) {
  return <aside className="details" aria-label="Device details">
    <div className="section-head"><h2>Device details</h2><button onClick={close}>Close</button></div>
    {!group ? <p>This device is no longer present in the latest successful scan.</p> : <>
      <DeviceIcon kind={group.members[0].class} />
      <h3>{group.members[0].name}</h3>
      <p className="muted">Information only. Hardware configuration unavailable.</p>
      {stale && <p className="warning">Stale metadata. Current presence unknown.</p>}
      {group.members.map((device: Device) => <details key={device.instance_id} open={group.members.length === 1}>
        <summary>{device.name}</summary>
        <dl>{[
          ['Class', device.class], ['Manufacturer', device.manufacturer], ['OS enumerator', device.enumerator],
          ['Instance ID', device.instance_id], ['Container ID', device.container_id], ['Parent', device.parent],
        ].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{show(value)}</dd></div>)}
          {device.usb_ids && <div><dt>USB identifiers</dt><dd>{device.usb_ids.map((id, i) => `${i ? 'PID' : 'VID'} ${id.toString(16).padStart(4, '0').toUpperCase()}`).join(' · ')}</dd></div>}
          <div><dt>Hardware IDs</dt><dd>{device.hardware_ids.length ? device.hardware_ids.map(id => <code key={id}>{id}</code>) : 'Unknown'}</dd></div>
        </dl>
      </details>)}
    </>}
  </aside>;
}

function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>(emptySnapshot);
  const [query, setQuery] = useState('');
  const [deviceClass, setClass] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [preferences, setPreferences] = useState<Preferences>(defaults);
  const [preferenceError, setPreferenceError] = useState<string | null>(null);
  const [saveMessage, setSaveMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const search = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const pending = useRef(false);

  async function refresh() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      if (!window.deviceLab) throw new Error('Desktop bridge unavailable. Launch with npm start, not a browser.');
      setSnapshot(await window.deviceLab.scan());
    } catch (error) {
      setSnapshot(previous => ({ ...previous, error: String(error) }));
    } finally { pending.current = false; setBusy(false); }
  }
  useEffect(() => {
    void refresh();
    const interval = window.setInterval(() => { void refresh(); }, 2000);
    window.deviceLab?.appearance().then(result => {
      setPreferences(result.preferences);
      setPreferenceError(result.error);
    }).catch(error => setPreferenceError(String(error)));
    return () => clearInterval(interval);
  }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = preferences.light ? 'light' : 'dark';
    document.documentElement.style.fontSize = `${preferences.scale * 16}px`;
  }, [preferences]);
  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (dialog.current?.open) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault(); search.current?.focus();
      }
      if (event.key === 'Escape') { if (selected) setSelected(null); else setQuery(''); }
    };
    window.addEventListener('keydown', keyboard);
    return () => window.removeEventListener('keydown', keyboard);
  }, [selected]);

  async function save() {
    if (!window.deviceLab) return;
    setSaving(true);
    try { setSaveMessage(await window.deviceLab.saveAppearance(preferences)); }
    catch (error) { setSaveMessage(String(error)); }
    finally { setSaving(false); }
  }
  const classes = [...new Set(snapshot.groups.flatMap(g => g.members.map(d => d.class)).filter(Boolean))].sort();
  const cards = snapshot.groups.filter(g => matches(g, query, deviceClass));
  return <>
    <header className="topbar">
      <div className="brand"><span className="brand-mark" aria-hidden="true">D</span> DEVICE LAB</div>
      <button className="tab active" onClick={() => setSelected(null)}>Dashboard</button>
      <span className="stack">ELECTRON / REACT / RUST</span>
      <div className="actions"><button disabled={busy} onClick={() => { void refresh(); }}>{busy ? 'Scanning…' : 'Refresh'}</button>
        <button onClick={() => dialog.current?.showModal()}>Appearance</button></div>
    </header>
    <div className={`workspace ${selected ? 'has-details' : ''}`}>
      <nav className="sidebar" aria-label="Device class filters">
        <p className="eyebrow">LIBRARY</p>
        <button className={!deviceClass ? 'filter selected' : 'filter'} aria-pressed={!deviceClass} onClick={() => setClass('')}>All Windows devices</button>
        <p className="eyebrow class-label">DEVICE CLASSES</p>
        {deviceClass && !classes.includes(deviceClass) && <button className="filter selected" onClick={() => setClass('')}>{deviceClass} (unavailable)</button>}
        {classes.map(name => <button key={name} className={deviceClass === name ? 'filter selected' : 'filter'} aria-pressed={deviceClass === name} onClick={() => setClass(name)}>{name}</button>)}
        <p className="sidebar-note">Any vendor.<br />OS metadata only.<br />No hardware commands.</p>
      </nav>
      <main>
        <div className="intro"><p className="eyebrow">YOUR WORKSPACE</p><h1>Your devices<span>.</span></h1>
          <p className="muted">Explore the hardware Windows currently exposes.</p></div>
        <label className="search"><span aria-hidden="true">⌕</span><input ref={search} value={query} onChange={event => setQuery(event.target.value)} placeholder="Search devices, classes or identifiers" aria-label="Search devices" /><kbd>Ctrl F</kbd></label>
        <div className="inventory-status" role="status">
          {snapshot.error ? <p className="warning">{snapshot.hasSnapshot ? 'Last scan failed; showing stale data. ' : 'Discovery unavailable. '}{snapshot.error}</p>
            : !snapshot.hasSnapshot ? <p>Discovering devices…</p>
            : <p>{snapshot.recordCount} OS records · {snapshot.groups.length} groups <span className="changes">+{snapshot.changes.added} added / {snapshot.changes.updated} updated / −{snapshot.changes.removed} removed</span></p>}
        </div>
        <div className="section-head"><h2>{deviceClass || 'All devices'}</h2><span className="muted">{cards.length} groups</span></div>
        {cards.length === 0 && snapshot.hasSnapshot && <p className="empty">No devices match this view.</p>}
        <div className="cards">{cards.map(group => {
          const device = group.members[0];
          return <button className={`device-card ${selected === group.key ? 'chosen' : ''}`} key={group.key} aria-pressed={selected === group.key} onClick={() => setSelected(group.key)}>
            <span className="card-kind">{show(device.class)}</span>
            <DeviceIcon kind={device.class} />
            <strong>{device.name}</strong><span className="manufacturer">{show(device.manufacturer)}</span>
            <span className="card-footer">Information only<span>{group.members.length} OS record{group.members.length === 1 ? '' : 's'}</span></span>
          </button>;
        })}</div>
      </main>
      {selected && <Details group={snapshot.groups.find(g => g.key === selected)} stale={!!snapshot.error} close={() => setSelected(null)} />}
    </div>
    <footer>Read-only OS metadata <span>Bluetooth presence ≠ connection status · Unpaired BLE / network discovery excluded</span></footer>
    <dialog ref={dialog} aria-labelledby="appearance-title">
      <div className="section-head"><h2 id="appearance-title">Appearance</h2><button onClick={() => dialog.current?.close()}>Close</button></div>
      <p className="muted">Make this workspace yours.</p>
      <label className="setting">Light theme<input type="checkbox" checked={preferences.light} onChange={event => setPreferences(p => ({ ...p, light: event.target.checked }))} /></label>
      <label className="setting" htmlFor="scale">UI scale <output>{Math.round(preferences.scale * 100)}%</output></label>
      <input id="scale" type="range" min="0.8" max="1.6" step="0.05" value={preferences.scale} onChange={event => setPreferences(p => ({ ...p, scale: Number(event.target.value) }))} />
      {preferenceError && <p className="warning">{preferenceError}</p>}
      <button className="primary" disabled={!!preferenceError || saving} onClick={() => { void save(); }}>{saving ? 'Saving…' : 'Save locally'}</button>
      <p role="status" className="muted">{saveMessage || 'Preferences only. No inventory is saved.'}</p>
    </dialog>
  </>;
}

createRoot(document.getElementById('root')!).render(<App />);
