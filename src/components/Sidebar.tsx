import { useState } from 'react';
import { Category, Machine, MachineState, MachineStatus } from '../api';
import { Dict } from '../i18n';
import { IconChip, IconGamepad, IconMonitor, IconPlus, IconSearch } from '../icons';
import { CATEGORIES, categoryLabel } from '../util';

function catIcon(c: Category) {
  if (c === 'console') return <IconGamepad size={13} />;
  if (c === 'modern') return <IconMonitor size={13} />;
  return <IconChip size={13} />;
}

export function Sidebar({
  machines,
  status,
  t,
  selected,
  onSelect,
  onNew,
}: {
  machines: Machine[];
  status: Record<string, MachineStatus>;
  t: Dict;
  selected: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
}) {
  const [q, setQ] = useState('');
  const query = q.trim().toLowerCase();
  const visible = query ? machines.filter((m) => m.name.toLowerCase().includes(query)) : machines;

  return (
    <aside className="sidebar">
      <div className="sb-title">
        <span className="grow">{t.machines}</span>
        <button className="icon" title={t.newMachineTitle} onClick={onNew}>
          <IconPlus size={11} />
        </button>
      </div>
      <div className="sb-search">
        <IconSearch size={12} />
        <input type="text" placeholder={t.search} value={q} onChange={(e) => setQ(e.target.value)} />
      </div>
      {machines.length === 0 && <div className="note" style={{ padding: '0 12px' }}>{t.noMachines}</div>}
      {CATEGORIES.map((c) => {
        const items = visible.filter((m) => m.category === c).sort((a, b) => a.name.localeCompare(b.name));
        if (items.length === 0) return null;
        return (
          <div key={c}>
            <div className="sb-title sb-cat">
              {catIcon(c)}
              <span className="grow">{categoryLabel(t, c)}</span>
              <span className="cnt">{items.length}</span>
            </div>
            {items.map((m) => {
              const st: MachineState = status[m.id]?.state ?? 'stopped';
              return (
                <button
                  key={m.id}
                  className={`sb-item ${selected === m.id ? 'active' : ''}`}
                  onClick={() => onSelect(m.id)}
                  title={m.name}
                >
                  <span className={`led ${st}`} />
                  <span className="fname">{m.name}</span>
                </button>
              );
            })}
          </div>
        );
      })}
      <div className="sb-foot">
        <button className="primary" onClick={onNew}>
          {t.newMachine}
        </button>
      </div>
    </aside>
  );
}
