import { useCallback, useEffect, useRef, useState } from 'react';
import { EngineInfo, Machine, MachineStatus, Profile, Settings, UpdateInfo, api, defaultSettings } from './api';
import { dicts, Lang } from './i18n';
import { Help } from './components/Help';
import { MachineDetail } from './components/MachineDetail';
import { SettingsModal } from './components/SettingsModal';
import { Sidebar } from './components/Sidebar';
import { Wizard } from './components/Wizard';
import { IconGear } from './icons';

const LOG_MAX = 500;

/** Bestätigungsdialog (window.confirm ist im WebView nicht verlässlich). */
function ConfirmModal({
  text,
  okLabel,
  cancelLabel,
  extra,
  onOk,
  onClose,
}: {
  text: string;
  okLabel: string;
  cancelLabel: string;
  extra?: JSX.Element;
  onOk: () => void;
  onClose: () => void;
}) {
  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="note" style={{ marginTop: 0, fontSize: 12.5, color: 'var(--text)' }}>
          {text}
        </div>
        {extra}
        <div className="btnrow">
          <button className="ghost" onClick={onClose} autoFocus>
            {cancelLabel}
          </button>
          <button className="primary" onClick={onOk}>
            {okLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [settingsBackup, setSettingsBackup] = useState<Settings | null>(null);
  const [machines, setMachines] = useState<Machine[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [status, setStatus] = useState<Record<string, MachineStatus>>({});
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const [loaded, setLoaded] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [showWizard, setShowWizard] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [confirm, setConfirm] = useState<{ text: string; extra?: JSX.Element } | null>(null);
  const [helpSignal, setHelpSignal] = useState(0);
  const [toast, setToast] = useState<string | null>(null);
  const [updateAvail, setUpdateAvail] = useState<UpdateInfo | null>(null);
  const [installing, setInstalling] = useState(false);
  const toastTimer = useRef<number | undefined>(undefined);
  const deleteFilesRef = useRef(true);

  const lang: Lang = settings?.language ?? 'de';
  const t = dicts[lang];
  const nested = settings?.nested ?? false;

  const showToast = useCallback((msg: string, isError = false) => {
    setToast(msg);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), isError ? 6000 : 2200);
  }, []);
  const fail = useCallback((e: unknown) => showToast(String(e), true), [showToast]);

  // Bestätigung als Promise
  const confirmResolve = useRef<((ok: boolean) => void) | null>(null);
  const onConfirm = useCallback(
    (text: string, extra?: JSX.Element) =>
      new Promise<boolean>((resolve) => {
        confirmResolve.current = resolve;
        setConfirm({ text, extra });
      }),
    []
  );
  const settleConfirm = (ok: boolean) => {
    setConfirm(null);
    confirmResolve.current?.(ok);
    confirmResolve.current = null;
  };

  const loadMachines = useCallback(
    () =>
      api
        .listMachines()
        .then((ms) => {
          setMachines(ms);
          setLoaded(true);
        })
        .catch(fail),
    [fail]
  );
  const loadStatus = useCallback(
    () =>
      api
        .allStatus()
        .then((list) => setStatus(Object.fromEntries(list.map((s) => [s.id, s]))))
        .catch(() => {}),
    []
  );
  const detect = useCallback(() => api.detectEngines().then(setEngines).catch(() => setEngines([])), []);

  // --- Laden -------------------------------------------------------------
  useEffect(() => {
    api
      .getSettings()
      .then((s) => {
        setSettings(s);
        api
          .checkUpdate()
          .then((u) => {
            if (!u) return;
            setUpdateAvail(u);
            if (s.autoUpdate) {
              setInstalling(true);
              api.installUpdate().catch(() => setInstalling(false));
            }
          })
          .catch(() => {});
      })
      .catch(() => setSettings({ ...defaultSettings }));
    api.listProfiles().then(setProfiles).catch(fail);
    loadMachines();
    loadStatus();
    detect();
  }, [fail, loadMachines, loadStatus, detect]);

  // Ereignisse vom Backend
  useEffect(() => {
    let offStatus: (() => void) | undefined;
    let offLog: (() => void) | undefined;
    api
      .onStatus((e) => {
        setStatus((s) => ({ ...s, [e.id]: { ...(s[e.id] ?? { id: e.id, pid: null, since: null }), id: e.id, state: e.state, error: e.error } }));
        if (e.state === 'error' && e.error) showToast(e.error, true);
      })
      .then((f) => (offStatus = f))
      .catch(() => {});
    api
      .onLog((e) => {
        setLogs((l) => {
          const lines = [...(l[e.id] ?? []), e.line];
          return { ...l, [e.id]: lines.length > LOG_MAX ? lines.slice(lines.length - LOG_MAX) : lines };
        });
      })
      .then((f) => (offLog = f))
      .catch(() => {});
    return () => {
      offStatus?.();
      offLog?.();
    };
  }, [showToast]);

  // Auswahl: erste Maschine, wenn nichts gewählt
  useEffect(() => {
    if (machines.length === 0) {
      setSelected(null);
      return;
    }
    if (!selected || !machines.some((m) => m.id === selected)) setSelected(machines[0].id);
  }, [machines, selected]);

  // Log der gewählten Maschine nachladen (z. B. nach Neustart der App)
  useEffect(() => {
    if (!selected) return;
    api
      .machineLog(selected)
      .then((lines) => setLogs((l) => ({ ...l, [selected]: lines })))
      .catch(() => {});
  }, [selected]);

  // Darstellung aus den Einstellungen auf <html> spiegeln
  useEffect(() => {
    if (!settings) return;
    document.documentElement.setAttribute('data-theme', settings.theme);
    document.documentElement.setAttribute('data-accent', settings.accent);
    document.documentElement.lang = settings.language;
  }, [settings]);

  // --- Aktionen ------------------------------------------------------------
  const saveSettings = (s: Settings) => {
    api
      .setSettings(s)
      .then(() => {
        setSettings(s);
        setSettingsBackup(null);
        setShowSettings(false);
        detect();
        loadMachines();
      })
      .catch(fail);
  };

  const toggleNested = () => {
    if (!settings) return;
    saveSettings({ ...settings, nested: !settings.nested });
  };

  const onMachine = (m: Machine) => setMachines((ms) => ms.map((x) => (x.id === m.id ? m : x)));

  const deleteMachine = async (m: Machine) => {
    deleteFilesRef.current = true;
    const extra = (
      <label className="check">
        <input
          type="checkbox"
          defaultChecked
          onChange={(e) => {
            deleteFilesRef.current = e.target.checked;
          }}
        />
        {t.deleteFilesToo}
      </label>
    );
    const ok = settings?.confirmDangerous === false ? true : await onConfirm(t.confirmDelete(m.name), extra);
    if (!ok) return;
    api
      .deleteMachine(m.id, deleteFilesRef.current)
      .then(() => {
        setMachines((ms) => ms.filter((x) => x.id !== m.id));
        setSelected(null);
      })
      .catch(fail);
  };

  const confirmDangerous = useCallback(
    (text: string) => (settings?.confirmDangerous === false ? Promise.resolve(true) : onConfirm(text)),
    [settings, onConfirm]
  );

  // --- Tastaturkürzel ------------------------------------------------------
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if ((e.metaKey || e.ctrlKey) && e.key === ',') {
        e.preventDefault();
        if (settings) {
          setSettingsBackup(settings);
          setShowSettings(true);
        }
        return;
      }
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (showSettings || showWizard || confirm) {
        if (e.key === 'Escape') {
          if (showSettings) {
            if (settingsBackup) setSettings(settingsBackup);
            setSettingsBackup(null);
            setShowSettings(false);
          }
          setShowWizard(false);
          if (confirm) settleConfirm(false);
        }
        return;
      }
      switch (e.key) {
        case 'n':
          setShowWizard(true);
          break;
        case '?':
          setHelpSignal((n) => n + 1);
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [settings, settingsBackup, showSettings, showWizard, confirm]);

  if (!settings) return null;

  const current = selected ? machines.find((m) => m.id === selected) ?? null : null;
  const currentStatus: MachineStatus = (current && status[current.id]) ?? { id: current?.id ?? '', state: 'stopped', pid: null, since: null, error: null };

  return (
    <div className="app">
      <header className="header">
        <span className="brand">
          <span className="name">virtual</span>
          <span className="dot">.</span>
        </span>
        <span className="tagline">{t.tagline}</span>
        <span className="grow" />
        <button className={`ghost nested-toggle ${nested ? 'on' : ''}`} title={t.nestedHint} onClick={toggleNested}>
          {nested ? <span className="badge">{t.nestedBadge}</span> : <span className="badge dimmed">nested</span>}
        </button>
        <button className="ghost icon hdr-help" title={t.help} onClick={() => setHelpSignal((n) => n + 1)}>
          ?
        </button>
        <button
          className="ghost icon"
          title={t.settings}
          onClick={() => {
            setSettingsBackup(settings);
            setShowSettings(true);
          }}
        >
          <IconGear size={14} />
        </button>
      </header>

      <div className="main">
        <Sidebar machines={machines} status={status} t={t} selected={selected} onSelect={setSelected} onNew={() => setShowWizard(true)} />

        {loaded && machines.length === 0 ? (
          <div className="view">
            <div className="onboard">
              <h2>{t.onboardTitle}</h2>
              <p>{t.onboardText}</p>
              <button className="primary" onClick={() => setShowWizard(true)}>
                {t.newMachine}
              </button>
            </div>
          </div>
        ) : current ? (
          <MachineDetail
            key={current.id}
            machine={current}
            profile={profiles.find((p) => p.id === current.profileId) ?? null}
            status={currentStatus}
            logLines={logs[current.id] ?? []}
            nested={nested}
            t={t}
            lang={lang}
            onMachine={onMachine}
            onDelete={deleteMachine}
            onToast={showToast}
            onFail={fail}
            onConfirm={confirmDangerous}
            onClearLog={() => setLogs((l) => ({ ...l, [current.id]: [] }))}
          />
        ) : (
          <div className="view" />
        )}
      </div>

      {showWizard && (
        <Wizard
          profiles={profiles}
          machines={machines}
          engines={engines}
          nested={nested}
          t={t}
          onCreated={(m) => {
            setMachines((ms) => [...ms, m]);
            setSelected(m.id);
            setShowWizard(false);
            showToast(t.wzCreated);
          }}
          onClose={() => setShowWizard(false)}
          onFail={fail}
        />
      )}

      {showSettings && (
        <SettingsModal
          settings={settings}
          engines={engines}
          t={t}
          onClose={() => {
            if (settingsBackup) setSettings(settingsBackup);
            setSettingsBackup(null);
            setShowSettings(false);
          }}
          onSave={saveSettings}
          onLive={(s) => setSettings(s)}
          onRescan={async (s) => {
            await api.setSettings(s);
            await detect();
          }}
          onFail={fail}
        />
      )}

      {confirm && (
        <ConfirmModal
          text={confirm.text}
          extra={confirm.extra}
          okLabel={t.ok}
          cancelLabel={t.cancel}
          onOk={() => settleConfirm(true)}
          onClose={() => settleConfirm(false)}
        />
      )}

      {updateAvail && (
        <div className="upd-banner">
          <span>
            {t.updateBanner} <strong>{updateAvail.version}</strong>
          </span>
          <button
            className="primary"
            disabled={installing}
            onClick={() => {
              setInstalling(true);
              api.installUpdate().catch(() => setInstalling(false));
            }}
          >
            {installing ? t.updateInstalling : t.updateInstall}
          </button>
          <button className="ghost" onClick={() => setUpdateAvail(null)}>
            {t.updateLater}
          </button>
        </div>
      )}

      <Help lang={lang} openSignal={helpSignal} />
      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
