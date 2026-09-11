// Vertrag zwischen React-UI und Tauri-Backend (Kommandos in src-tauri/src/commands.rs).
// Typen spiegeln virtual-core/src/model.rs (serde camelCase).

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export const isTauri = '__TAURI_INTERNALS__' in window;

export type Engine = 'qemu' | 'libretro';
export type Arch = 'x86_64' | 'i386' | 'aarch64' | 'm68k' | 'ppc' | 'riscv64' | 'mips' | 'none';
export type Firmware = 'bios' | 'uefi' | 'uefi-secure';
export type NetworkMode = 'off' | 'isolated' | 'nat' | 'bridged';
export type MediaKind = 'cdrom' | 'floppy' | 'rom' | 'physical';
export type Category = 'modern' | 'retro-pc' | 'retro-mac' | 'other-arch' | 'console';
export type MachineState = 'stopped' | 'starting' | 'running' | 'paused' | 'error';

export interface PortForward {
  proto: 'tcp' | 'udp';
  hostPort: number;
  guestPort: number;
}
export interface Network {
  mode: NetworkMode;
  forwards: PortForward[];
  proxy: boolean;
}
export interface Cpu {
  model: string;
  cores: number;
  clockPercent: number;
}
export interface Devices {
  graphics: string;
  audio: string[];
  nic: string;
  storageBus: string;
  floppy: boolean;
  input: string;
}
export interface DiskRef {
  id: string;
  path: string;
  format: string;
  sizeGb: number;
  encrypted: boolean;
  backing: string | null;
}
export interface MediaRef {
  id: string;
  kind: MediaKind;
  path: string;
  slot: number;
}
export interface SharedFolder {
  hostPath: string;
  name: string;
  readOnly: boolean;
}
export interface Isolation {
  throwaway: boolean;
  readOnlyShares: boolean;
}
export interface Display {
  kind: 'window' | 'headless' | 'vnc' | 'spice';
  gl: boolean;
  hidpi: boolean;
  fullscreen: boolean;
}
export interface LibretroConfig {
  core: string;
  fallbackCore: string | null;
  bios: string[];
  stateSlot: number;
}
export type BootDevice = 'auto' | 'disk' | 'cdrom' | 'floppy';

export interface Machine {
  id: string;
  name: string;
  profileId: string;
  category: Category;
  engine: Engine;
  arch: Arch;
  machineType: string;
  cpu: Cpu;
  memoryMb: number;
  firmware: Firmware;
  tpm: boolean;
  disks: DiskRef[];
  media: MediaRef[];
  network: Network;
  display: Display;
  devices: Devices;
  usb: string[];
  sharedFolders: SharedFolder[];
  isolation: Isolation;
  libretro: LibretroConfig | null;
  /** auto | disk | cdrom | floppy */
  boot: BootDevice;
  extraArgs: string[];
  notes: string;
  createdAt: string;
  updatedAt: string;
}
export interface Snapshot {
  id: string;
  machineId: string;
  parentId: string | null;
  name: string;
  note: string;
  createdAt: string;
  kind: 'internal' | 'state' | 'overlay';
  tag: string;
}
export interface MachineStatus {
  id: string;
  state: MachineState;
  pid: number | null;
  since: string | null;
  error: string | null;
}
export interface EngineInfo {
  engine: 'qemu' | 'qemu-img' | 'retroarch' | 'swtpm';
  binary: string;
  path: string | null;
  version: string | null;
  ok: boolean;
  accel: string | null;
  hint: string;
}
export interface ProfileOs {
  family: string;
  versions: string[];
  hint: string;
}
export interface ProfileLibretro {
  core: string;
  fallbackCore: string | null;
  bios: string[];
  extensions: string[];
}
export interface Profile {
  id: string;
  name: string;
  epoch: number;
  category: Category;
  os: ProfileOs;
  engine: Engine;
  arch: Arch;
  machineType: string;
  cpu: Cpu;
  memoryMb: number;
  firmware: Firmware;
  tpm: boolean;
  devices: Devices;
  network: Network;
  isolation: Isolation;
  display: Display;
  tier: 'free' | 'nested';
  beta: boolean;
  notes: string;
  defaultDiskGb: number;
  needsRom: boolean;
  libretro: ProfileLibretro | null;
}

export interface Settings {
  language: 'de' | 'en';
  theme: 'dark' | 'light';
  accent: 'blue' | 'emerald' | 'violet' | 'amber';
  autoUpdate: boolean;
  /** Tarif „nested“ — im Vorabzugang frei umschaltbar. */
  nested: boolean;
  /** Ordner mit den Maschinenordnern (leer = App-Datenordner/machines). */
  machinesDir: string;
  /** Pfad-Überschreibungen für Engines (leer = automatisch suchen). */
  qemuDir: string;
  retroarchPath: string;
  coresDir: string;
  systemDir: string;
  /** Vor dem Löschen und harten Stoppen nachfragen. */
  confirmDangerous: boolean;
}

export interface CreateMachineInput {
  profileId: string;
  name: string;
  /** Systemplatte in GB (0 = keine). */
  diskGb: number;
  /** Installationsmedium / ROM / Disc-Abbild (optional). */
  mediaPath: string | null;
  /** Firmware-ROM (q800) (optional). */
  romPath: string | null;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}

/** Ereignisse vom Backend. */
export interface StatusEvent {
  id: string;
  state: MachineState;
  error: string | null;
}
export interface LogEvent {
  id: string;
  line: string;
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export const api = {
  // Einstellungen
  getSettings: () => call<Settings>('get_settings'),
  setSettings: (settings: Settings) => call<void>('set_settings', { settings }),
  dataPath: () => call<string>('data_path'),

  // Profile & Maschinen
  listProfiles: () => call<Profile[]>('list_profiles'),
  listMachines: () => call<Machine[]>('list_machines'),
  getMachine: (id: string) => call<Machine>('get_machine', { id }),
  createMachine: (input: CreateMachineInput) => call<Machine>('create_machine', { input }),
  saveMachine: (machine: Machine) => call<Machine>('save_machine', { machine }),
  deleteMachine: (id: string, deleteFiles: boolean) => call<void>('delete_machine', { id, deleteFiles }),
  machineDir: (id: string) => call<string>('machine_dir', { id }),
  /** Kommandozeile, die beim Start ausgeführt würde (Nerd-Ansicht). */
  previewCommand: (id: string) => call<string>('preview_command', { id }),

  // Laufzeit
  startMachine: (id: string) => call<MachineStatus>('start_machine', { id }),
  /** force = Prozess beenden statt ACPI-Ausschalten / QUIT. */
  stopMachine: (id: string, force: boolean) => call<MachineStatus>('stop_machine', { id, force }),
  pauseMachine: (id: string) => call<MachineStatus>('pause_machine', { id }),
  resumeMachine: (id: string) => call<MachineStatus>('resume_machine', { id }),
  resetMachine: (id: string) => call<MachineStatus>('reset_machine', { id }),
  machineStatus: (id: string) => call<MachineStatus>('machine_status', { id }),
  allStatus: () => call<MachineStatus[]>('all_status'),
  machineLog: (id: string) => call<string[]>('machine_log', { id }),

  // Medien
  attachMedia: (id: string, kind: MediaKind, path: string, slot: number) =>
    call<Machine>('attach_media', { id, kind, path, slot }),
  detachMedia: (id: string, mediaId: string) => call<Machine>('detach_media', { id, mediaId }),

  // Snapshots
  listSnapshots: (id: string) => call<Snapshot[]>('list_snapshots', { id }),
  createSnapshot: (id: string, name: string, note: string) => call<Snapshot>('create_snapshot', { id, name, note }),
  restoreSnapshot: (id: string, snapshotId: string) => call<void>('restore_snapshot', { id, snapshotId }),
  deleteSnapshot: (id: string, snapshotId: string) => call<void>('delete_snapshot', { id, snapshotId }),

  // Engines
  detectEngines: () => call<EngineInfo[]>('detect_engines'),
  /** Fehlende BIOS-Dateien einer Konsolen-Maschine (leer = alles da). */
  missingBios: (id: string) => call<string[]>('missing_bios', { id }),

  // App
  openPath: (path: string) => call<void>('open_path', { path }),
  checkUpdate: () => call<UpdateInfo | null>('check_update'),
  installUpdate: () => call<void>('install_update'),

  // Ereignisse
  onStatus: (cb: (e: StatusEvent) => void): Promise<UnlistenFn> =>
    listen<StatusEvent>('machine-status', (ev) => cb(ev.payload)),
  onLog: (cb: (e: LogEvent) => void): Promise<UnlistenFn> => listen<LogEvent>('machine-log', (ev) => cb(ev.payload)),
};

export const defaultSettings: Settings = {
  language: 'de',
  theme: 'dark',
  accent: 'blue',
  autoUpdate: false,
  nested: false,
  machinesDir: '',
  qemuDir: '',
  retroarchPath: '',
  coresDir: '',
  systemDir: '',
  confirmDangerous: true,
};
