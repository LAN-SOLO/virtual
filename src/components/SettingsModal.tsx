import { useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { EngineInfo, Settings, UpdateInfo, api } from '../api';
import { Dict } from '../i18n';
import { IconCheck, IconExternalLink, IconWarn } from '../icons';

export const APP_VERSION = '0.1.0';

type SetTab = 'general' | 'engines' | 'folders' | 'app';

const WEBSITE = 'https://lan-solo.com/de/tools/virtual/';
const GITHUB = 'https://github.com/LAN-SOLO/virtual';

export function SettingsModal({
  settings,
  engines,
  t,
  onClose,
  onSave,
  onLive,
  onRescan,
  onFail,
}: {
  settings: Settings;
  engines: EngineInfo[];
  t: Dict;
  onClose: () => void;
  onSave: (s: Settings) => void;
  onLive: (s: Settings) => void;
  onRescan: (s: Settings) => Promise<void>;
  onFail: (e: unknown) => void;
}) {
  const [tab, setTab] = useState<SetTab>('general');
  const [s, setS] = useState<Settings>({ ...settings });
  const [updState, setUpdState] = useState<'idle' | 'checking' | 'none' | 'error'>('idle');
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);
  const [dataPath, setDataPath] = useState('');
  const [scanning, setScanning] = useState(false);

  useEffect(() => {
    api.dataPath().then(setDataPath).catch(() => {});
  }, []);

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setS((prev) => {
      const next = { ...prev, [key]: value };
      onLive(next);
      return next;
    });

  const pickDir = async (key: 'machinesDir' | 'qemuDir' | 'coresDir' | 'systemDir') => {
    const sel = await open({ directory: true, multiple: false });
    if (typeof sel === 'string') set(key, sel);
  };
  const pickFile = async (key: 'retroarchPath') => {
    const sel = await open({ directory: false, multiple: false });
    if (typeof sel === 'string') set(key, sel);
  };

  const rescan = () => {
    setScanning(true);
    onRescan(s)
      .catch(onFail)
      .finally(() => setScanning(false));
  };

  const checkUpdates = () => {
    setUpdState('checking');
    setUpdate(null);
    api
      .checkUpdate()
      .then((u) => {
        if (u) {
          setUpdate(u);
          setUpdState('idle');
        } else setUpdState('none');
      })
      .catch(() => setUpdState('error'));
  };

  const tabs: [SetTab, string][] = [
    ['general', t.setGeneral],
    ['engines', t.setEngines],
    ['folders', t.setFolders],
    ['app', t.setApp],
  ];

  const pathField = (label: string, key: 'qemuDir' | 'coresDir' | 'systemDir' | 'machinesDir' | 'retroarchPath', file = false) => (
    <div className="field">
      <span className="fieldlabel" style={{ marginTop: 0 }}>
        {label}
      </span>
      <div className="pickrow">
        <input type="text" value={s[key]} placeholder="auto" onChange={(e) => set(key, e.target.value)} />
        <button onClick={() => (file ? pickFile(key as 'retroarchPath') : pickDir(key as 'machinesDir'))}>{file ? t.pickFile : t.pickFolder}</button>
        {s[key] && (
          <button className="ghost" onClick={() => set(key, '')}>
            ×
          </button>
        )}
      </div>
    </div>
  );

  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal settings" onClick={(e) => e.stopPropagation()}>
        <h2>{t.settings}</h2>
        <div className="set-tabs">
          {tabs.map(([id, label]) => (
            <button key={id} className={`chip ${tab === id ? 'active' : ''}`} onClick={() => setTab(id)}>
              {label}
            </button>
          ))}
        </div>

        {tab === 'general' && (
          <>
            <div className="row3">
              <label className="field grow1">
                <span>{t.language}</span>
                <select value={s.language} onChange={(e) => set('language', e.target.value as Settings['language'])}>
                  <option value="de">Deutsch</option>
                  <option value="en">English</option>
                </select>
              </label>
              <label className="field grow1">
                <span>{t.theme}</span>
                <select value={s.theme} onChange={(e) => set('theme', e.target.value as Settings['theme'])}>
                  <option value="dark">{t.themeDark}</option>
                  <option value="light">{t.themeLight}</option>
                </select>
              </label>
              <label className="field grow1">
                <span>{t.accent}</span>
                <select value={s.accent} onChange={(e) => set('accent', e.target.value as Settings['accent'])}>
                  <option value="blue">{t.accentBlue}</option>
                  <option value="emerald">{t.accentEmerald}</option>
                  <option value="violet">{t.accentViolet}</option>
                  <option value="amber">{t.accentAmber}</option>
                </select>
              </label>
            </div>
            <label className="check">
              <input type="checkbox" checked={s.confirmDangerous} onChange={(e) => set('confirmDangerous', e.target.checked)} />
              {t.confirmDangerous}
            </label>
            <div className="sep" />
            <label className="check">
              <input type="checkbox" checked={s.nested} onChange={(e) => set('nested', e.target.checked)} />
              {t.nestedToggle} <span className="badge">nested</span>
            </label>
            <div className="note">{t.nestedExplain}</div>
            <div className="note">{t.nestedHint}</div>
          </>
        )}

        {tab === 'engines' && (
          <>
            <div className="note">{t.enginesIntro}</div>
            <div className="englist">
              {engines.map((e) => (
                <div key={e.engine} className={`engrow ${e.ok ? 'ok' : 'bad'}`}>
                  <span className="engstate">{e.ok ? <IconCheck size={12} /> : <IconWarn size={12} />}</span>
                  <span className="engname mono">{e.binary}</span>
                  <span className={`chip mini ${e.ok ? 'ok' : 'bad'}`}>{e.ok ? t.engineFound : t.engineMissing}</span>
                  {e.version && <span className="chip mini dim">v{e.version}</span>}
                  {e.accel && (
                    <span className="chip mini dim">
                      {t.engineAccel}: {e.accel}
                    </span>
                  )}
                  <span className="engpath mono dim" title={e.path ?? ''}>
                    {e.path ?? ''}
                  </span>
                  {!e.ok && e.hint && (
                    <span className="enghint mono">
                      {t.engineInstall}: {e.hint}
                    </span>
                  )}
                </div>
              ))}
            </div>
            <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
              <button onClick={rescan} disabled={scanning}>
                {scanning ? t.checking : t.rescan}
              </button>
            </div>
            <div className="sep" />
            {pathField(t.qemuDir, 'qemuDir')}
            {pathField(t.retroarchPath, 'retroarchPath', true)}
            {pathField(t.coresDir, 'coresDir')}
            {pathField(t.systemDir, 'systemDir')}
          </>
        )}

        {tab === 'folders' && (
          <>
            {pathField(t.machinesDir, 'machinesDir')}
            <div className="note">{t.machinesDirHint}</div>
            <div className="fieldlabel">{t.currentDataPath}</div>
            <div className="note mono" style={{ userSelect: 'text', WebkitUserSelect: 'text' }}>
              {dataPath}
            </div>
          </>
        )}

        {tab === 'app' && (
          <>
            <div className="fieldlabel">{t.updates}</div>
            <label className="check">
              <input type="checkbox" checked={s.autoUpdate} onChange={(e) => set('autoUpdate', e.target.checked)} />
              {t.autoUpdate}
            </label>
            <div className="updatebox">
              <span>
                {t.version} {APP_VERSION}
              </span>
              <button onClick={checkUpdates} disabled={updState === 'checking'}>
                {updState === 'checking' ? t.checking : t.checkUpdates}
              </button>
              {updState === 'none' && <span>{t.upToDate}</span>}
              {updState === 'error' && <span style={{ color: 'var(--red)' }}>{t.updateError}</span>}
              {update && (
                <>
                  <span>
                    {t.updateAvailable} <strong>{update.version}</strong>
                  </span>
                  <button
                    className="primary"
                    disabled={installing}
                    onClick={() => {
                      setInstalling(true);
                      api.installUpdate().catch(() => setInstalling(false));
                    }}
                  >
                    {installing ? t.updateInstalling : t.installUpdate}
                  </button>
                </>
              )}
            </div>
            <div className="sep" />
            <div className="btnrow" style={{ justifyContent: 'flex-start' }}>
              <button onClick={() => api.openPath(WEBSITE).catch(onFail)}>
                <IconExternalLink size={11} /> {t.website}
              </button>
              <button onClick={() => api.openPath(GITHUB).catch(onFail)}>
                <IconExternalLink size={11} /> {t.github}
              </button>
            </div>
            <div className="note">{t.licenseNote}</div>
            <div className="note">{t.shortcuts}</div>
          </>
        )}

        <div className="btnrow">
          <button className="ghost" onClick={onClose}>
            {t.cancel}
          </button>
          <button className="primary" onClick={() => onSave(s)}>
            {t.saveSettings}
          </button>
        </div>
      </div>
    </div>
  );
}
