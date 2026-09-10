import { useMemo, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Category, CreateMachineInput, EngineInfo, Machine, Profile, api } from '../api';
import { Dict } from '../i18n';
import { IconCheck, IconWarn } from '../icons';
import { CATEGORIES, baseName, categoryLabel, extensionsForProfile, fmtMem, suggestName } from '../util';

export function Wizard({
  profiles,
  machines,
  engines,
  nested,
  t,
  onCreated,
  onClose,
  onFail,
}: {
  profiles: Profile[];
  machines: Machine[];
  engines: EngineInfo[];
  nested: boolean;
  t: Dict;
  onCreated: (m: Machine) => void;
  onClose: () => void;
  onFail: (e: unknown) => void;
}) {
  const [step, setStep] = useState(0);
  const [cat, setCat] = useState<Category>('modern');
  const [q, setQ] = useState('');
  const [profile, setProfile] = useState<Profile | null>(null);
  const [name, setName] = useState('');
  const [diskGb, setDiskGb] = useState(0);
  const [mediaPath, setMediaPath] = useState<string | null>(null);
  const [romPath, setRomPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const query = q.trim().toLowerCase();
  const list = useMemo(
    () =>
      profiles.filter((p) => {
        if (query) {
          const hay = [p.name, p.os.family, ...p.os.versions].join(' ').toLowerCase();
          return hay.includes(query);
        }
        return p.category === cat;
      }),
    [profiles, cat, query]
  );

  const engineInfo = (p: Profile): EngineInfo | undefined =>
    engines.find((e) => e.engine === (p.engine === 'qemu' ? 'qemu' : 'retroarch'));
  const engineMissing = profile ? engineInfo(profile)?.ok === false : false;

  const pick = (p: Profile) => {
    setProfile(p);
    setName(suggestName(p.os.versions[0] ?? p.name, machines));
    setDiskGb(p.defaultDiskGb);
    setMediaPath(null);
    setRomPath(null);
    setStep(1);
  };

  const chooseMedia = async () => {
    if (!profile) return;
    const ext = extensionsForProfile(profile);
    const sel = await open({ multiple: false, directory: false, filters: [{ name: profile.libretro ? 'ROM' : 'ISO/IMG', extensions: ext }] });
    if (typeof sel === 'string') setMediaPath(sel);
  };
  const chooseRom = async () => {
    const sel = await open({ multiple: false, directory: false });
    if (typeof sel === 'string') setRomPath(sel);
  };

  const create = () => {
    if (!profile) return;
    setBusy(true);
    const input: CreateMachineInput = { profileId: profile.id, name: name.trim() || profile.name, diskGb, mediaPath, romPath };
    api
      .createMachine(input)
      .then((m) => {
        onCreated(m);
      })
      .catch((e) => {
        setBusy(false);
        onFail(e);
      });
  };

  const steps = [t.wzStep1, t.wzStep2, t.wzStep3];
  const isConsole = profile?.engine === 'libretro';

  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal wizard" onClick={(e) => e.stopPropagation()}>
        <h2>{t.newMachineTitle}</h2>
        <div className="wz-steps">
          {steps.map((s, i) => (
            <span key={s} className={`wz-step ${i === step ? 'on' : ''} ${i < step ? 'done' : ''}`}>
              <span className="wz-num">{i < step ? <IconCheck size={10} /> : i + 1}</span>
              {s}
            </span>
          ))}
        </div>

        {step === 0 && (
          <>
            <div className="set-tabs">
              {CATEGORIES.map((c) => (
                <button
                  key={c}
                  className={`chip ${cat === c && !query ? 'active' : ''}`}
                  onClick={() => {
                    setCat(c);
                    setQ('');
                  }}
                >
                  {categoryLabel(t, c)}
                </button>
              ))}
              <input type="text" className="wz-search" placeholder={t.wzSearch} value={q} onChange={(e) => setQ(e.target.value)} />
            </div>
            <div className="wz-list">
              {list.length === 0 && <div className="note">{t.wzNoProfiles}</div>}
              {list.map((p) => {
                const info = engineInfo(p);
                return (
                  <button key={p.id} className="wz-card" onClick={() => pick(p)}>
                    <div className="wz-card-head">
                      <span className="wz-card-name">{p.name}</span>
                      <span className="chip mini">{p.epoch > 0 ? p.epoch : t.wzEpochNow}</span>
                      {p.tier === 'nested' && <span className="badge">nested</span>}
                      {p.beta && <span className="chip mini warn">{t.wzBeta}</span>}
                      {info && !info.ok && (
                        <span className="chip mini bad" title={info.hint}>
                          <IconWarn size={10} /> {info.engine}
                        </span>
                      )}
                    </div>
                    <div className="wz-card-family">{p.os.family}</div>
                    <div className="wz-card-versions">
                      {p.os.versions.map((v) => (
                        <span key={v} className="chip mini dim">
                          {v}
                        </span>
                      ))}
                    </div>
                    <div className="wz-card-hint">{p.notes || p.os.hint}</div>
                  </button>
                );
              })}
            </div>
          </>
        )}

        {step === 1 && profile && (
          <>
            <div className="wz-selected">
              <span className="wz-card-name">{profile.name}</span>
              <span className="dim"> · {profile.os.family}</span>
            </div>
            {engineMissing && (
              <div className="note warn">
                <IconWarn size={12} /> {t.wzEngineMissing(engineInfo(profile)?.binary ?? profile.engine, engineInfo(profile)?.hint ?? '')}
              </div>
            )}
            {profile.tier === 'nested' && !nested && <div className="note">{t.wzNestedProfile}</div>}
            <div className="row2">
              <label className="field grow1">
                <span>{t.wzName}</span>
                <input type="text" value={name} autoFocus onChange={(e) => setName(e.target.value)} />
              </label>
              {profile.defaultDiskGb > 0 && !isConsole && (
                <label className="field" style={{ width: 140 }}>
                  <span>{t.wzDisk}</span>
                  <input
                    type="number"
                    min={0}
                    max={4096}
                    value={diskGb}
                    onChange={(e) => setDiskGb(Math.max(0, Math.min(4096, Number(e.target.value) || 0)))}
                  />
                </label>
              )}
            </div>
            <div className="fieldlabel">
              {isConsole ? t.wzGame : t.wzMedia} <span className="dim">({t.wzOptional})</span>
            </div>
            <div className="pickrow">
              <button onClick={chooseMedia}>{t.choose}</button>
              <span className="pickval mono">{mediaPath ? baseName(mediaPath) : '—'}</span>
              {mediaPath && (
                <button className="ghost" onClick={() => setMediaPath(null)}>
                  ×
                </button>
              )}
            </div>
            {profile.needsRom && (
              <>
                <div className="fieldlabel">{t.wzRom}</div>
                <div className="pickrow">
                  <button onClick={chooseRom}>{t.choose}</button>
                  <span className="pickval mono">{romPath ? baseName(romPath) : '—'}</span>
                  {romPath && (
                    <button className="ghost" onClick={() => setRomPath(null)}>
                      ×
                    </button>
                  )}
                </div>
                <div className="note">{t.wzNeedsRom}</div>
              </>
            )}
            <div className="note">
              <strong>{t.wzHint}:</strong> {profile.os.hint}
            </div>
            {profile.libretro && profile.libretro.bios.length > 0 && (
              <div className="note">
                BIOS: <span className="mono">{profile.libretro.bios.join(', ')}</span>
              </div>
            )}
          </>
        )}

        {step === 2 && profile && (
          <>
            <div className="note" style={{ color: 'var(--text)' }}>
              {t.wzSummary}
            </div>
            <dl className="facts">
              <dt>{t.wzName}</dt>
              <dd>{name.trim() || profile.name}</dd>
              <dt>{t.profile}</dt>
              <dd>{profile.name}</dd>
              <dt>{t.engine}</dt>
              <dd>{profile.engine === 'qemu' ? t.engineQemu : t.engineLibretro}</dd>
              {!isConsole && (
                <>
                  <dt>{t.machineType}</dt>
                  <dd className="mono">
                    {profile.arch} / {profile.machineType}
                  </dd>
                  <dt>{t.cpu}</dt>
                  <dd className="mono">
                    {profile.cpu.model} · {profile.cpu.cores} {t.cores} · {profile.cpu.clockPercent} %
                  </dd>
                  <dt>{t.memory}</dt>
                  <dd>{fmtMem(profile.memoryMb)}</dd>
                  <dt>{t.wzDisk}</dt>
                  <dd>{diskGb > 0 ? `${diskGb} GB` : t.none}</dd>
                  <dt>{t.network}</dt>
                  <dd className="mono">{profile.network.mode}</dd>
                </>
              )}
              {isConsole && profile.libretro && (
                <>
                  <dt>{t.core}</dt>
                  <dd className="mono">{profile.libretro.core}</dd>
                </>
              )}
              <dt>{isConsole ? t.wzGame : t.wzMedia}</dt>
              <dd className="mono">{mediaPath ? baseName(mediaPath) : '—'}</dd>
              {profile.needsRom && (
                <>
                  <dt>{t.wzRom}</dt>
                  <dd className="mono">{romPath ? baseName(romPath) : '—'}</dd>
                </>
              )}
            </dl>
          </>
        )}

        <div className="btnrow">
          <button className="ghost" onClick={onClose}>
            {t.cancel}
          </button>
          {step > 0 && <button onClick={() => setStep(step - 1)}>{t.back}</button>}
          {step === 1 && (
            <button className="primary" onClick={() => setStep(2)} disabled={!name.trim()}>
              {t.next}
            </button>
          )}
          {step === 2 && (
            <button className="primary" onClick={create} disabled={busy}>
              {t.create}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
