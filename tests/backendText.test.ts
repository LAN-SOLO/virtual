// Prüft die Übersetzung der Backend-Meldungen (src/backendText.ts):
//   node --test --experimental-strip-types tests/   (bzw. pnpm test:ui)
// Der Abdeckungstest liest alle deutschen Meldungen aus src-tauri/src und core/src
// (ohne Testmodule) und verlangt, dass jede eine englische Übersetzung bekommt —
// eine neue Meldung ohne Muster lässt diesen Test scheitern.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { trBackend, trLines, trLogLine } from '../src/backendText.ts';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function rustFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? rustFiles(p) : p.endsWith('.rs') ? [p] : [];
  });
}

/** Deutsche Meldungstexte (String-Literale in Err/ok_or/format!/into) aus dem Nicht-Test-Code. */
function backendMessages(): string[] {
  const out = new Set<string>();
  const files = [...rustFiles(join(root, 'src-tauri/src')), ...rustFiles(join(root, 'core/src'))];
  for (const f of files) {
    let src = readFileSync(f, 'utf8');
    const cut = src.indexOf('#[cfg(test)]');
    if (cut >= 0) src = src.slice(0, cut);
    // nur Code, keine Kommentare
    src = src
      .split('\n')
      .filter((l) => !l.trim().startsWith('//'))
      .join('\n');
    const re = /(?:Err\(|ok_or\(|ok_or_else\(\|\||format!\(|=>|push\(|: &str =|\|_\|)\s*"((?:[^"\\]|\\.)*)"/g;
    for (const m of src.matchAll(re)) {
      const lit = m[1].replace(/\\"/g, '"').replace(/\\n/g, '\n');
      if (!/[äöüÄÖÜß]|\b(nicht|fehlt|keine?|Maschine|Platte|Abbild|bereits|läuft|lesen|senden|anlegen|schreiben|starten)\b/.test(lit)) continue;
      out.add(lit.replace(/\{[^}]*\}/g, 'X'));
    }
  }
  return [...out];
}

test('jede deutsche Backend-Meldung hat eine englische Übersetzung', () => {
  const msgs = backendMessages();
  assert.ok(msgs.length > 60, `nur ${msgs.length} Meldungen gefunden — Extraktion kaputt?`);
  const missing = msgs.filter((m) => {
    const en = trBackend(m, 'en');
    return en === m || /[äöüß„]/.test(en.replace(/X/g, ''));
  });
  assert.deepEqual(missing, [], 'ohne Übersetzung:\n' + missing.join('\n'));
});

test('Deutsch bleibt unverändert', () => {
  assert.equal(trBackend('Maschine läuft bereits', 'de'), 'Maschine läuft bereits');
});

test('eingebettete Ursachen werden mitübersetzt, Engine-Text bleibt', () => {
  assert.equal(
    trBackend('Abbild nicht lesbar: CUE: Datendatei /x/Spiel.bin fehlt', 'en'),
    'Image not readable: CUE: data file /x/Spiel.bin is missing'
  );
  assert.equal(
    trBackend('qemu-system-i386 nicht gefunden — winget install SoftwareFreedomConservancy.QEMU', 'en'),
    'qemu-system-i386 not found — winget install SoftwareFreedomConservancy.QEMU'
  );
  assert.equal(trBackend('qemu-img: Could not open disk-0.qcow2', 'en'), 'qemu-img: Could not open disk-0.qcow2');
  assert.equal(trBackend(new Error('Kein Update verfügbar'), 'en'), 'No update available');
  assert.equal(trBackend('irgendwas Unbekanntes', 'en'), 'irgendwas Unbekanntes');
});

test('Hinweiszeilen in Log und Vorschau', () => {
  assert.equal(trLogLine('# TPM 2.0: swtpm nicht gefunden — die Maschine startet ohne TPM.', 'en'), '# TPM 2.0: swtpm not found — the machine starts without a TPM.');
  assert.equal(trLogLine('qemu-system-x86_64: -device foo: nicht gefunden', 'en'), 'qemu-system-x86_64: -device foo: nicht gefunden');
  assert.equal(
    trLines('$ qemu-system-x86_64 -m 512\n# OpenGL: von diesem QEMU-Build nicht unterstützt — Anzeige ohne GL.', 'en'),
    '$ qemu-system-x86_64 -m 512\n# OpenGL: not supported by this QEMU build — display without GL.'
  );
});
