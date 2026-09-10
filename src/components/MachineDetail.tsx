import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  Firmware,
  Machine,
  MachineStatus,
  MediaKind,
  NetworkMode,
  PortForward,
  Profile,
  Snapshot,
  api,
} from '../api';
import { Dict, Lang } from '../i18n';
import { IconCamera, IconFolder, IconPause, IconPlay, IconReset, IconStop, IconTrash, IconWarn } from '../icons';
import {
  AUDIO_OPTIONS,
  BUS_OPTIONS,
  CPU_OPTIONS,
  GRAPHICS_OPTIONS,
  INPUT_OPTIONS,
  NIC_OPTIONS,
  baseName,
  categoryLabel,
  extensionsForProfile,
  fmtDateTime,
  fmtMem,
  stateLabel,
} from '../util';

type Tab = 'overview' | 'hardware' | 'media' | 'network' | 'snapshots' | 'log';

export function MachineDetail({
  machine,
  profile,
  status,
  logLines,
  nested,
  t,
  lang,
  onMachine,
  onDelete,
  onToast,
  onFail,
  onConfirm,
  onClearLog,
}: {
  machine: Machine;
  profile: Profile | null;
  status: MachineStatus;
  logLines: string[];
  nested: boolean;
  t: Dict;
  lang: Lang;
  onMachine: (m: Machine) => void;
  onDelete: (m: Machine) => void;
  onToast: (msg: string) => void;
  onFail: (e: unknown) => void;
  onConfirm: (text: string) => Promise<boolean>;
  onClearLog: () => void;
}) {
  const [tab, setTab] = useState<Tab>('overview');
  const [draft, setDraft] = useState<Machine>(machine);
  const [dirty, setDirty] = useState(false);
  const [command, setCommand] = useState('');
  const [dir, setDir] = useState('');
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [snapModal, setSnapModal] = useState(false);
  const [snapName, setSnapName] = useState('');
  const [snapNote, setSnapNote] = useState('');
  const [missingBios, setMissingBios] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  const st = status.state;
  const running = st === 'running' || st === 'paused' || st === 'starting';
  const isConsole = machine.engine === 'libretro';

  // Maschine gewechselt oder von außen aktualisiert → Entwurf verwerfen
  useEffect(() => {
    setDraft(machine);
    setDirty(false);
  }, [machine]);

  useEffect(() => {
    api.previewCommand(machine.id).then(setCommand).catch(() => setCommand(''));
    api.machineDir(machine.id).then(setDir).catch(() => setDir(''));
  }, [machine]);

  useEffect(() => {
    if (tab === 'snapshots') api.listSnapshots(machine.id).then(setSnapshots).catch(onFail);
    if (tab === 'media' && isConsole) api.missingBios(machine.id).then(setMissingBios).catch(() => setMissingBios(null));
  }, [tab, machine.id, isConsole, onFail, st]);

  useEffect(() => {
    if (tab === 'log' && logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [logLines, tab]);

  const edit = (patch: Partial<Machine>) => {
    setDraft((d) => ({ ...d, ...patch }));
    setDirty(true);
  };

  const run = (p: Promise<MachineStatus>) => {
    setBusy(true);
    p.catch(onFail).finally(() => setBusy(false));
  };

  const save = () => {
    api
      .saveMachine(draft)
      .then((m) => {
        onMachine(m);
        setDirty(false);
        onToast(t.saved);
      })
      .catch(onFail);
  };

  const attach = async (kind: MediaKind, slot: number) => {
    const ext = profile ? extensionsForProfile(profile) : undefined;
    const filters =
      kind === 'floppy'
        ? [{ name: 'IMG', extensions: ['img', 'ima', 'dsk', 'vfd'] }]
        : kind === 'cdrom'
          ? [{ name: 'ISO', extensions: ['iso', 'bin', 'cue', 'img', 'dmg', 'toast'] }]
          : ext
            ? [{ name: 'ROM', extensions: ext }]
            : undefined;
    const sel = await open({ multiple: false, directory: false, filters });
    if (typeof sel !== 'string') return;
    api.attachMedia(machine.id, kind, sel, slot).then(onMachine).catch(onFail);
  };

  const createSnapshot = () => {
    api
      .createSnapshot(machine.id, snapName.trim() || `Snapshot ${snapshots.length + 1}`, snapNote.trim())
      .then((s) => {
        setSnapshots((l) => [...l, s]);
        setSnapModal(false);
        setSnapName('');
        setSnapNote('');
      })
      .catch(onFail);
  };

  const tabs: [Tab, string][] = [
    ['overview', t.tabOverview],
    ['hardware', t.tabHardware],
    ['media', t.tabMedia],
    ['network', t.tabNetwork],
    ['snapshots', t.tabSnapshots],
    ['log', t.tabLog],
  ];

  const cpuOptions = CPU_OPTIONS[machine.arch] ?? [];
  const netModes: [NetworkMode, string, string][] = [
    ['off', t.netOff, t.netOffHint],
    ['isolated', t.netIsolated, t.netIsolatedHint],
    ['nat', t.netNat, t.netNatHint],
    ['bridged', t.netBridged, t.netBridgedHint],
  ];

  return (
    <div className="view">
      <div className="viewbar">
        <h1>{machine.name}</h1>
        <span className="sub">{profile?.name ?? machine.profileId}</span>
        <span className="chip mini">{categoryLabel(t, machine.category)}</span>
        <span className={`chip mini state-${st}`}>
          <span className={`led ${st}`} /> {stateLabel(t, st)}
        </span>
        <span className="grow" />
        {!running && (
          <button className="primary" disabled={busy} onClick={() => run(api.startMachine(machine.id))}>
            <IconPlay size={11} /> {t.start}
          </button>
        )}
        {st === 'running' && (
          <button disabled={busy} onClick={() => run(api.pauseMachine(machine.id))}>
            <IconPause size={11} /> {t.pause}
          </button>
        )}
        {st === 'paused' && (
          <button className="primary" disabled={busy} onClick={() => run(api.resumeMachine(machine.id))}>
            <IconPlay size={11} /> {t.resume}
          </button>
        )}
        {running && (
          <button disabled={busy || st === 'starting'} onClick={() => run(api.resetMachine(machine.id))}>
            <IconReset size={11} /> {t.reset}
          </button>
        )}
        {running && (
          <button disabled={busy} onClick={() => run(api.stopMachine(machine.id, false))} title={t.stopHint}>
            <IconStop size={11} /> {t.stop}
          </button>
        )}
        {running && (
          <button
            className="danger"
            disabled={busy}
            onClick={async () => {
              if (await onConfirm(t.confirmForce(machine.name))) run(api.stopMachine(machine.id, true));
            }}
          >
            {t.stopForce}
          </button>
        )}
      </div>
      {status.error && (
        <div className="errbar">
          <IconWarn size={12} /> {status.error}
        </div>
      )}
      <div className="tabs detail-tabs">
        {tabs.map(([id, label]) => (
          <button key={id} className={`chip ${tab === id ? 'active' : ''}`} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
        <span className="grow" />
        {dirty && <span className="dim" style={{ fontSize: 11 }}>{t.unsaved}</span>}
        {(tab === 'hardware' || tab === 'network' || tab === 'overview') && (
          <button className="primary" disabled={!dirty || running} onClick={save} title={running ? t.runningNoEdit : ''}>
            {t.save}
          </button>
        )}
      </div>

      <div className="viewbody">
        {running && tab !== 'log' && tab !== 'snapshots' && tab !== 'overview' && <div className="note">{t.runningNoEdit}</div>}

        {tab === 'overview' && (
          <>
            <dl className="facts">
              <dt>{t.engine}</dt>
              <dd>{isConsole ? t.engineLibretro : t.engineQemu}</dd>
              {isConsole && machine.libretro ? (
                <>
                  <dt>{t.core}</dt>
                  <dd className="mono">
                    {machine.libretro.core}
                    {machine.libretro.fallbackCore && <span className="dim"> · {t.fallbackCore}: {machine.libretro.fallbackCore}</span>}
                  </dd>
                </>
              ) : (
                <>
                  <dt>{t.arch}</dt>
                  <dd className="mono">
                    {machine.arch} / {machine.machineType}
                  </dd>
                  <dt>{t.cpu}</dt>
                  <dd className="mono">
                    {machine.cpu.model} · {machine.cpu.cores} {t.cores} · {t.clock} {machine.cpu.clockPercent} %
                  </dd>
                  <dt>{t.memory}</dt>
                  <dd>{fmtMem(machine.memoryMb)}</dd>
                  <dt>{t.firmware}</dt>
                  <dd className="mono">
                    {machine.firmware}
                    {machine.tpm && ` · ${t.tpm}`}
                  </dd>
                  <dt>{t.network}</dt>
                  <dd className="mono">{machine.network.mode}</dd>
                </>
              )}
              <dt>{t.isolation}</dt>
              <dd>
                {[machine.isolation.throwaway && t.throwaway, machine.isolation.readOnlyShares && t.readOnlyShares]
                  .filter(Boolean)
                  .join(' · ') || '—'}
              </dd>
              <dt>{t.created}</dt>
              <dd>{fmtDateTime(machine.createdAt, lang)}</dd>
            </dl>
            <label className="field">
              <span>{t.notes}</span>
              <textarea rows={3} value={draft.notes} onChange={(e) => edit({ notes: e.target.value })} />
            </label>
            <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
              <button onClick={() => dir && api.openPath(dir).catch(onFail)} disabled={!dir}>
                <IconFolder size={12} /> {t.openFolder}
              </button>
              <span className="dim mono" style={{ fontSize: 11, alignSelf: 'center', userSelect: 'text' }}>
                {dir}
              </span>
            </div>
            <div className="fieldlabel">{t.commandPreview}</div>
            <pre className="cmd">{command || '—'}</pre>
            <div className="note">{t.commandHint}</div>
          </>
        )}

        {tab === 'hardware' && (
          <>
            {isConsole ? (
              <div className="note">{t.hwConsoleHint}</div>
            ) : (
              <>
                <div className="row3">
                  <label className="field grow1">
                    <span>{t.hwCpuModel}</span>
                    <input
                      type="text"
                      list="cpu-models"
                      value={draft.cpu.model}
                      disabled={running}
                      onChange={(e) => edit({ cpu: { ...draft.cpu, model: e.target.value } })}
                    />
                    <datalist id="cpu-models">
                      {cpuOptions.map((c) => (
                        <option key={c} value={c} />
                      ))}
                    </datalist>
                  </label>
                  <label className="field">
                    <span>{t.hwCores}</span>
                    <input
                      type="number"
                      min={1}
                      max={64}
                      value={draft.cpu.cores}
                      disabled={running}
                      onChange={(e) => edit({ cpu: { ...draft.cpu, cores: Math.max(1, Math.min(64, Number(e.target.value) || 1)) } })}
                    />
                  </label>
                  <label className="field">
                    <span>{t.hwMemoryMb}</span>
                    <input
                      type="number"
                      min={1}
                      max={262144}
                      value={draft.memoryMb}
                      disabled={running}
                      onChange={(e) => edit({ memoryMb: Math.max(1, Number(e.target.value) || 1) })}
                    />
                  </label>
                </div>
                <label className="field">
                  <span>
                    {t.hwClock} · <span className="mono">{draft.cpu.clockPercent} %</span>
                  </span>
                  <input
                    type="range"
                    min={10}
                    max={100}
                    step={5}
                    value={draft.cpu.clockPercent}
                    disabled={running}
                    onChange={(e) => edit({ cpu: { ...draft.cpu, clockPercent: Number(e.target.value) } })}
                  />
                </label>
                <div className="note">{t.hwClockHint}</div>
                <div className="row2">
                  <label className="field grow1">
                    <span>{t.hwFirmware}</span>
                    <select value={draft.firmware} disabled={running} onChange={(e) => edit({ firmware: e.target.value as Firmware })}>
                      <option value="bios">{t.fwBios}</option>
                      <option value="uefi">{t.fwUefi}</option>
                      <option value="uefi-secure">{t.fwUefiSecure}</option>
                    </select>
                  </label>
                  <label className="check grow1" style={{ alignSelf: 'end' }}>
                    <input type="checkbox" checked={draft.tpm} disabled={running} onChange={(e) => edit({ tpm: e.target.checked })} />
                    {t.hwTpm}
                  </label>
                </div>
                <div className="row3">
                  <label className="field grow1">
                    <span>{t.hwGraphics}</span>
                    <input
                      type="text"
                      list="gfx"
                      value={draft.devices.graphics}
                      disabled={running}
                      onChange={(e) => edit({ devices: { ...draft.devices, graphics: e.target.value } })}
                    />
                    <datalist id="gfx">
                      {GRAPHICS_OPTIONS.map((c) => (
                        <option key={c} value={c} />
                      ))}
                    </datalist>
                  </label>
                  <label className="field grow1">
                    <span>{t.hwAudio}</span>
                    <input
                      type="text"
                      list="audio"
                      value={draft.devices.audio.join(', ')}
                      disabled={running}
                      onChange={(e) =>
                        edit({
                          devices: {
                            ...draft.devices,
                            audio: e.target.value
                              .split(',')
                              .map((s) => s.trim())
                              .filter(Boolean),
                          },
                        })
                      }
                    />
                    <datalist id="audio">
                      {AUDIO_OPTIONS.map((c) => (
                        <option key={c} value={c} />
                      ))}
                    </datalist>
                  </label>
                  <label className="field grow1">
                    <span>{t.hwNic}</span>
                    <input
                      type="text"
                      list="nic"
                      value={draft.devices.nic}
                      disabled={running}
                      onChange={(e) => edit({ devices: { ...draft.devices, nic: e.target.value } })}
                    />
                    <datalist id="nic">
                      {NIC_OPTIONS.map((c) => (
                        <option key={c} value={c} />
                      ))}
                    </datalist>
                  </label>
                </div>
                <div className="row3">
                  <label className="field grow1">
                    <span>{t.hwStorageBus}</span>
                    <select
                      value={draft.devices.storageBus}
                      disabled={running}
                      onChange={(e) => edit({ devices: { ...draft.devices, storageBus: e.target.value } })}
                    >
                      {BUS_OPTIONS.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field grow1">
                    <span>{t.hwInput}</span>
                    <select
                      value={draft.devices.input}
                      disabled={running}
                      onChange={(e) => edit({ devices: { ...draft.devices, input: e.target.value } })}
                    >
                      {INPUT_OPTIONS.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="check grow1" style={{ alignSelf: 'end' }}>
                    <input
                      type="checkbox"
                      checked={draft.devices.floppy}
                      disabled={running}
                      onChange={(e) => edit({ devices: { ...draft.devices, floppy: e.target.checked } })}
                    />
                    {t.hwFloppy}
                  </label>
                </div>
              </>
            )}
            <div className="row3">
              {!isConsole && (
                <label className="field grow1">
                  <span>{t.hwDisplay}</span>
                  <select
                    value={draft.display.kind}
                    disabled={running}
                    onChange={(e) => edit({ display: { ...draft.display, kind: e.target.value as Machine['display']['kind'] } })}
                  >
                    <option value="window">{t.dispWindow}</option>
                    <option value="headless">{t.dispHeadless}</option>
                    <option value="vnc">{t.dispVnc}</option>
                  </select>
                </label>
              )}
              <label className="check grow1" style={{ alignSelf: 'end' }}>
                <input
                  type="checkbox"
                  checked={draft.display.gl}
                  disabled={running}
                  onChange={(e) => edit({ display: { ...draft.display, gl: e.target.checked } })}
                />
                {t.hwGl}
              </label>
              <label className="check grow1" style={{ alignSelf: 'end' }}>
                <input
                  type="checkbox"
                  checked={draft.display.fullscreen}
                  disabled={running}
                  onChange={(e) => edit({ display: { ...draft.display, fullscreen: e.target.checked } })}
                />
                {t.hwFullscreen}
              </label>
            </div>
            <div className="sep" />
            <label className="check">
              <input
                type="checkbox"
                checked={draft.isolation.throwaway}
                disabled={running}
                onChange={(e) => edit({ isolation: { ...draft.isolation, throwaway: e.target.checked } })}
              />
              {t.throwaway}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={draft.isolation.readOnlyShares}
                disabled={running}
                onChange={(e) => edit({ isolation: { ...draft.isolation, readOnlyShares: e.target.checked } })}
              />
              {t.readOnlyShares}
            </label>
            {!isConsole && (
              <>
                <div className="sep" />
                <label className="field">
                  <span>
                    {t.hwExtraArgs} {nested && <span className="badge">nested</span>}
                  </span>
                  <textarea
                    rows={3}
                    className="mono"
                    value={draft.extraArgs.join('\n')}
                    disabled={running || !nested}
                    onChange={(e) => edit({ extraArgs: e.target.value.split('\n').map((s) => s.trim()).filter(Boolean) })}
                  />
                </label>
                <div className="note">{nested ? t.hwExtraArgsHint : t.hwExtraArgsLocked}</div>
              </>
            )}
          </>
        )}

        {tab === 'media' && (
          <>
            {!isConsole && (
              <>
                <div className="fieldlabel">{t.disks}</div>
                {machine.disks.length === 0 && <div className="note">{t.noDisks}</div>}
                {machine.disks.map((d) => (
                  <div key={d.id} className="mrow">
                    <span className="chip mini">{d.format}</span>
                    <span className="mono grow">{d.path}</span>
                    <span className="dim">{d.sizeGb} GB</span>
                  </div>
                ))}
                <div className="sep" />
              </>
            )}
            <div className="fieldlabel">{isConsole ? t.game : t.media}</div>
            {machine.media.length === 0 && <div className="note">{t.noMedia}</div>}
            {machine.media.map((m) => (
              <div key={m.id} className="mrow">
                <span className="chip mini">
                  {m.kind === 'cdrom' ? t.kindCdrom : m.kind === 'floppy' ? t.kindFloppy : m.kind === 'rom' ? t.kindRom : t.kindPhysical}
                  {m.kind !== 'rom' && ` ${m.slot}`}
                </span>
                <span className="mono grow" title={m.path}>
                  {baseName(m.path)}
                </span>
                <button className="icon" disabled={running} title={t.remove} onClick={() => api.detachMedia(machine.id, m.id).then(onMachine).catch(onFail)}>
                  <IconTrash size={11} />
                </button>
              </div>
            ))}
            <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
              {isConsole ? (
                <button disabled={running} onClick={() => attach('rom', 0)}>
                  {t.attachGame}
                </button>
              ) : (
                <>
                  <button disabled={running} onClick={() => attach('cdrom', machine.media.filter((x) => x.kind === 'cdrom').length)}>
                    {t.attachCd}
                  </button>
                  <button disabled={running} onClick={() => attach('floppy', machine.media.some((x) => x.kind === 'floppy' && x.slot === 0) ? 1 : 0)}>
                    {t.attachFloppy}
                  </button>
                  <button disabled={running} onClick={() => attach('rom', 0)}>
                    {t.attachRom}
                  </button>
                </>
              )}
            </div>
            {!isConsole && <div className="note">{t.mediaHintQemu}</div>}
            {isConsole && machine.libretro && machine.libretro.bios.length > 0 && (
              <>
                <div className="sep" />
                {missingBios && missingBios.length === 0 && <div className="note ok">{t.biosOk}</div>}
                {missingBios && missingBios.length > 0 && (
                  <>
                    <div className="note warn">
                      <IconWarn size={12} /> {t.biosMissing}
                    </div>
                    <ul className="plain mono">
                      {missingBios.map((b) => (
                        <li key={b}>{b}</li>
                      ))}
                    </ul>
                    <div className="note">{t.biosHint}</div>
                  </>
                )}
              </>
            )}
          </>
        )}

        {tab === 'network' && (
          <>
            {isConsole ? (
              <div className="note">{t.netConsoleHint}</div>
            ) : (
              <>
                <div className="fieldlabel">{t.netMode}</div>
                {netModes.map(([mode, label, hint]) => (
                  <label key={mode} className="check radio">
                    <input
                      type="radio"
                      name="netmode"
                      checked={draft.network.mode === mode}
                      disabled={running}
                      onChange={() => edit({ network: { ...draft.network, mode } })}
                    />
                    <span>
                      {label}
                      <span className="dim block">{hint}</span>
                    </span>
                  </label>
                ))}
                <div className="sep" />
                <div className="fieldlabel">{t.forwards}</div>
                {draft.network.forwards.length === 0 && <div className="note">{t.noForwards}</div>}
                {draft.network.forwards.map((f, i) => (
                  <div key={i} className="mrow">
                    <select
                      value={f.proto}
                      disabled={running}
                      onChange={(e) => {
                        const forwards = draft.network.forwards.map((x, j) => (j === i ? { ...x, proto: e.target.value as PortForward['proto'] } : x));
                        edit({ network: { ...draft.network, forwards } });
                      }}
                    >
                      <option value="tcp">tcp</option>
                      <option value="udp">udp</option>
                    </select>
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      value={f.hostPort}
                      disabled={running}
                      title={t.fwHost}
                      onChange={(e) => {
                        const forwards = draft.network.forwards.map((x, j) => (j === i ? { ...x, hostPort: Number(e.target.value) || 0 } : x));
                        edit({ network: { ...draft.network, forwards } });
                      }}
                    />
                    <span className="dim">→</span>
                    <input
                      type="number"
                      min={1}
                      max={65535}
                      value={f.guestPort}
                      disabled={running}
                      title={t.fwGuest}
                      onChange={(e) => {
                        const forwards = draft.network.forwards.map((x, j) => (j === i ? { ...x, guestPort: Number(e.target.value) || 0 } : x));
                        edit({ network: { ...draft.network, forwards } });
                      }}
                    />
                    <button
                      className="icon"
                      disabled={running}
                      title={t.remove}
                      onClick={() => edit({ network: { ...draft.network, forwards: draft.network.forwards.filter((_, j) => j !== i) } })}
                    >
                      <IconTrash size={11} />
                    </button>
                  </div>
                ))}
                <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
                  <button
                    disabled={running}
                    onClick={() =>
                      edit({ network: { ...draft.network, forwards: [...draft.network.forwards, { proto: 'tcp', hostPort: 2222, guestPort: 22 }] } })
                    }
                  >
                    {t.add}
                  </button>
                </div>
                <div className="sep" />
                <div className="fieldlabel">{t.shares}</div>
                {draft.sharedFolders.length === 0 && <div className="note">{t.noShares}</div>}
                {draft.sharedFolders.map((s, i) => (
                  <div key={i} className="mrow">
                    <input
                      type="text"
                      value={s.name}
                      disabled={running}
                      title={t.shareName}
                      style={{ width: 120 }}
                      onChange={(e) => {
                        const sharedFolders = draft.sharedFolders.map((x, j) => (j === i ? { ...x, name: e.target.value.replace(/[^a-zA-Z0-9_-]/g, '') } : x));
                        edit({ sharedFolders });
                      }}
                    />
                    <span className="mono grow" title={s.hostPath}>
                      {s.hostPath}
                    </span>
                    <label className="check" style={{ margin: 0 }}>
                      <input
                        type="checkbox"
                        checked={s.readOnly}
                        disabled={running}
                        onChange={(e) => {
                          const sharedFolders = draft.sharedFolders.map((x, j) => (j === i ? { ...x, readOnly: e.target.checked } : x));
                          edit({ sharedFolders });
                        }}
                      />
                      {t.shareRo}
                    </label>
                    <button className="icon" disabled={running} title={t.remove} onClick={() => edit({ sharedFolders: draft.sharedFolders.filter((_, j) => j !== i) })}>
                      <IconTrash size={11} />
                    </button>
                  </div>
                ))}
                <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
                  <button
                    disabled={running}
                    onClick={async () => {
                      const sel = await open({ directory: true, multiple: false });
                      if (typeof sel !== 'string') return;
                      const name = baseName(sel).replace(/[^a-zA-Z0-9_-]/g, '') || `share${draft.sharedFolders.length}`;
                      edit({ sharedFolders: [...draft.sharedFolders, { hostPath: sel, name, readOnly: draft.isolation.readOnlyShares }] });
                    }}
                  >
                    {t.addShare}
                  </button>
                </div>
                <div className="note">{t.sharesHint}</div>
              </>
            )}
          </>
        )}

        {tab === 'snapshots' && (
          <>
            <div className="note">{isConsole ? t.snapHintConsole : t.snapHintQemu}</div>
            {snapshots.length === 0 && <div className="note">{t.noSnapshots}</div>}
            {snapshots.map((s) => (
              <div key={s.id} className="mrow snap">
                <IconCamera size={12} />
                <span className="grow">
                  <strong>{s.name}</strong>
                  {s.note && <span className="dim"> — {s.note}</span>}
                  <span className="dim block">
                    {fmtDateTime(s.createdAt, lang)} · {s.kind === 'internal' ? t.snapKindInternal : s.kind === 'state' ? t.snapKindState : t.snapKindOverlay} ·{' '}
                    <span className="mono">{s.tag}</span>
                  </span>
                </span>
                <button
                  disabled={busy || st === 'starting' || (isConsole && !running)}
                  onClick={async () => {
                    if (!(await onConfirm(t.confirmRestore(s.name)))) return;
                    setBusy(true);
                    api.restoreSnapshot(machine.id, s.id).then(() => onToast(t.ok)).catch(onFail).finally(() => setBusy(false));
                  }}
                >
                  {t.restore}
                </button>
                <button
                  className="icon"
                  disabled={busy}
                  title={t.delete}
                  onClick={async () => {
                    if (!(await onConfirm(t.confirmDeleteSnap(s.name)))) return;
                    api
                      .deleteSnapshot(machine.id, s.id)
                      .then(() => setSnapshots((l) => l.filter((x) => x.id !== s.id)))
                      .catch(onFail);
                  }}
                >
                  <IconTrash size={11} />
                </button>
              </div>
            ))}
            <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
              <button className="primary" disabled={busy || st === 'starting' || (isConsole && !running)} onClick={() => setSnapModal(true)}>
                <IconCamera size={11} /> {t.newSnapshot}
              </button>
            </div>
          </>
        )}

        {tab === 'log' && (
          <>
            <div className="btnrow" style={{ justifyContent: 'flex-start', marginTop: 0, marginBottom: 8 }}>
              <button className="ghost" onClick={onClearLog}>
                {t.logClear}
              </button>
            </div>
            <pre className="log" ref={logRef}>
              {logLines.length === 0 ? t.logEmpty : logLines.join('\n')}
            </pre>
          </>
        )}

        <div className="sep" style={{ marginTop: 32 }} />
        <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
          <button className="danger ghost" disabled={running} onClick={() => onDelete(machine)}>
            <IconTrash size={11} /> {t.deleteMachine}
          </button>
        </div>
      </div>

      {snapModal && (
        <div className="overlay" onClick={() => setSnapModal(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{t.newSnapshot}</h2>
            <label className="field">
              <span>{t.snapName}</span>
              <input type="text" value={snapName} autoFocus onChange={(e) => setSnapName(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && createSnapshot()} />
            </label>
            <label className="field">
              <span>{t.snapNote}</span>
              <input type="text" value={snapNote} onChange={(e) => setSnapNote(e.target.value)} />
            </label>
            <div className="btnrow">
              <button className="ghost" onClick={() => setSnapModal(false)}>
                {t.cancel}
              </button>
              <button className="primary" onClick={createSnapshot}>
                {t.create}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
