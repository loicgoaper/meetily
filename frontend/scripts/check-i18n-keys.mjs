// Meetily perso: every literal t('a.b.c') key used in src/ must exist in both
// fr and en dictionaries (plural keys may exist as key_one / key_other).
// Usage: node scripts/check-i18n-keys.mjs
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const SRC = new URL('../src/', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const LOCALES = join(SRC, 'i18n', 'locales');

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== 'node_modules' && name !== 'i18n') walk(p, out);
    } else if (/\.(tsx?|jsx?)$/.test(name)) {
      out.push(p);
    }
  }
  return out;
}

function loadLanguage(lang) {
  const dict = {};
  for (const file of readdirSync(join(LOCALES, lang))) {
    if (!file.endsWith('.json')) continue;
    dict[file.replace(/\.json$/, '')] = JSON.parse(readFileSync(join(LOCALES, lang, file), 'utf8'));
  }
  return dict;
}

function has(dict, key) {
  const parts = key.split('.');
  let node = dict;
  for (let i = 0; i < parts.length - 1; i++) {
    node = node?.[parts[i]];
    if (node === undefined) return false;
  }
  const last = parts[parts.length - 1];
  return node?.[last] !== undefined || node?.[`${last}_one`] !== undefined || node?.[`${last}_other`] !== undefined;
}

const dicts = { fr: loadLanguage('fr'), en: loadLanguage('en') };
const keyPattern = /\bt\(\s*['"`]([a-zA-Z0-9_.]+)['"`]/g;
const missing = [];
let used = 0;

for (const file of walk(SRC)) {
  const text = readFileSync(file, 'utf8');
  for (const match of text.matchAll(keyPattern)) {
    const key = match[1];
    if (!key.includes('.')) continue;
    used++;
    for (const lang of ['fr', 'en']) {
      if (!has(dicts[lang], key)) missing.push(`${lang}: ${key}  (${file.replace(SRC, 'src/')})`);
    }
  }
}

if (missing.length) {
  console.error(`Missing ${missing.length} translation(s):\n` + missing.join('\n'));
  process.exit(1);
}
console.log(`OK: ${used} key usages, all present in fr and en.`);
