// Kleine reine Helfer für die Oberfläche — keine Tauri-Abhängigkeit.

import { Category, Machine, MachineState, Profile } from './api';
import { Dict, Lang } from './i18n';

export const CATEGORIES: Category[] = ['modern', 'retro-pc', 'retro-mac', 'other-arch', 'console'];

export function categoryLabel(t: Dict, c: Category): string {
  switch (c) {
    case 'modern':
      return t.catModern;
    case 'retro-pc':
      return t.catRetroPc;
    case 'retro-mac':
      return t.catRetroMac;
    case 'other-arch':
      return t.catOther;
    case 'console':
      return t.catConsole;
  }
}

export function stateLabel(t: Dict, s: MachineState): string {
  switch (s) {
    case 'stopped':
      return t.stStopped;
    case 'starting':
      return t.stStarting;
    case 'running':
      return t.stRunning;
    case 'paused':
      return t.stPaused;
    case 'error':
      return t.stError;
  }
}

export function fmtDateTime(iso: string, lang: Lang): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(lang === 'de' ? 'de-DE' : 'en-GB', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function fmtMem(mb: number): string {
  return mb >= 1024 && mb % 256 === 0 ? `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB` : `${mb} MB`;
}

export function baseName(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i >= 0 ? path.slice(i + 1) : path;
}

export function profileName(profiles: Profile[], m: Machine): string {
  return profiles.find((p) => p.id === m.profileId)?.name ?? m.profileId;
}

/** Auswahllisten für die Hardware-Ansicht — sinnvolle QEMU-Gerätenamen je Bereich. */
export const GRAPHICS_OPTIONS = ['std', 'cirrus-vga', 'VGA', 'virtio-vga', 'virtio-vga-gl', 'virtio-gpu-pci', 'qxl-vga', 'vmware-svga', 'none'];
export const AUDIO_OPTIONS = ['sb16', 'adlib', 'gus', 'es1370', 'ac97', 'intel-hda', 'cs4231a'];
export const NIC_OPTIONS = ['virtio-net-pci', 'e1000', 'e1000e', 'rtl8139', 'ne2k_pci', 'ne2k_isa', 'pcnet', 'usb-net'];
export const BUS_OPTIONS = ['virtio', 'ide', 'sata', 'scsi'];
export const INPUT_OPTIONS = ['ps2', 'usb', 'virtio', 'adb'];
export const CPU_OPTIONS: Record<string, string[]> = {
  x86_64: ['host', 'max', 'qemu64', 'Skylake-Client', 'EPYC', 'Nehalem', 'core2duo'],
  i386: ['486', 'pentium', 'pentium2', 'pentium3', 'coreduo', 'qemu32', 'max'],
  aarch64: ['host', 'cortex-a72', 'cortex-a57', 'cortex-a53', 'max'],
  m68k: ['m68040', 'm68030', 'm68020'],
  ppc: ['g4', 'g3', '750', '7400', '7450'],
  riscv64: ['rv64', 'max'],
  mips: ['24Kf', '4Kc', '74Kf'],
  none: [],
};

export function extensionsForProfile(p: Profile): string[] {
  if (p.libretro) return p.libretro.extensions;
  return ['iso', 'nrg', 'bin', 'cue', 'mdf', 'mds', 'ccd', 'img', 'ima', 'dsk', 'cdr', 'dmg', 'toast'];
}

/** Eindeutigen Vorschlag für den Maschinennamen bilden. */
export function suggestName(base: string, machines: Machine[]): string {
  const taken = new Set(machines.map((m) => m.name.toLowerCase()));
  if (!taken.has(base.toLowerCase())) return base;
  for (let i = 2; i < 100; i++) {
    const n = `${base} ${i}`;
    if (!taken.has(n.toLowerCase())) return n;
  }
  return `${base} ${Date.now()}`;
}

/** CD-Abbilder, die beim Einlegen nach ISO gewandelt werden (Backend entscheidet endgültig). */
export const CD_EXTENSIONS = ['iso', 'nrg', 'bin', 'cue', 'mdf', 'mds', 'ccd', 'img', 'cdr', 'toast', 'dmg'];
export function mayNeedConversion(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return ['nrg', 'bin', 'cue', 'mdf', 'mds', 'ccd'].includes(ext);
}
