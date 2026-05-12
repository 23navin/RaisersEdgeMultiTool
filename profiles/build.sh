#!/usr/bin/env bash
# Repacks each profile in profiles/src/<name>/ into profiles/<name>.import.
# Each profile is verified before packing — the same shape the Rust backend
# (src-tauri/src/profile.rs) and DuckDB runner (src-tauri/src/db.rs) expect.
# Verifier uses node + js-yaml, both already in the project's npm deps.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="$SCRIPT_DIR/src"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

verify_profile() {
  local profile_dir="$1"
  ( cd "$REPO_ROOT" && node - "$profile_dir" ) <<'JS'
const fs = require('fs');
const path = require('path');
const yaml = require('js-yaml');

const profileDir = process.argv[2];
const errors = [];
const infos = [];
const err = (m) => errors.push(m);
const info = (m) => infos.push(m);

// ── file structure ──────────────────────────────────────────────────────────
const yamlPath = path.join(profileDir, 'structure.yaml');
if (!fs.existsSync(yamlPath)) {
  console.error('  x missing structure.yaml');
  process.exit(1);
}

let data;
try {
  data = yaml.load(fs.readFileSync(yamlPath, 'utf8'));
} catch (e) {
  console.error(`  x structure.yaml is not valid YAML: ${e.message}`);
  process.exit(1);
}

if (data === null || typeof data !== 'object' || Array.isArray(data)) {
  console.error('  x structure.yaml must contain a mapping at top level');
  process.exit(1);
}

const isStr = (v) => typeof v === 'string';
const isBool = (v) => typeof v === 'boolean';
const isInt = (v) => Number.isInteger(v);
const isList = (v) => Array.isArray(v);
const isMap = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

// ── top-level keys ──────────────────────────────────────────────────────────
for (const k of ['id', 'name', 'version', 'min_app_version', 'inputs', 'outputs', 'steps']) {
  if (!(k in data)) err(`structure.yaml missing required key: ${k}`);
}
for (const k of ['id', 'name', 'version', 'min_app_version']) {
  if (data[k] !== undefined && !isStr(data[k])) err(`${k} must be a string`);
}

// ── inputs ──────────────────────────────────────────────────────────────────
const inputLabels = new Set();
for (const [i, inp] of (data.inputs || []).entries()) {
  const where = `inputs[${i}]`;
  if (!isMap(inp)) { err(`${where} must be a mapping`); continue; }
  if (!isStr(inp.label) || !inp.label) {
    err(`${where}.label must be a non-empty string`);
  } else {
    if (inputLabels.has(inp.label)) err(`${where}.label '${inp.label}' is duplicated`);
    inputLabels.add(inp.label);
  }
  if (inp.type !== 'csv' && inp.type !== 'xlsx') {
    err(`${where}.type must be 'csv' or 'xlsx' (got ${JSON.stringify(inp.type)})`);
  }
  if (!isBool(inp.required)) err(`${where}.required must be true or false`);
  if (inp.validation !== undefined && inp.validation !== null) {
    if (!isList(inp.validation)) {
      err(`${where}.validation must be a list`);
    } else {
      for (const [j, rule] of inp.validation.entries()) {
        const rw = `${where}.validation[${j}]`;
        if (!isMap(rule)) { err(`${rw} must be a mapping`); continue; }
        if (!isStr(rule.label) || !rule.label) err(`${rw}.label must be a non-empty string`);
        if (!isBool(rule.required)) err(`${rw}.required must be true or false`);
        if (rule.type !== 'string' && rule.type !== 'number') {
          err(`${rw}.type must be 'string' or 'number' (got ${JSON.stringify(rule.type)})`);
        }
        if ('digits' in rule) {
          if (rule.type !== 'number') err(`${rw}.digits only allowed when type: number`);
          else if (!isInt(rule.digits) || rule.digits <= 0) err(`${rw}.digits must be a positive integer`);
        }
        if ('value' in rule) {
          if (!isList(rule.value) || !rule.value.every(isStr)) err(`${rw}.value must be a list of strings`);
        }
      }
    }
  }
}

// ── outputs ─────────────────────────────────────────────────────────────────
const outputLabels = new Set();
for (const [i, out] of (data.outputs || []).entries()) {
  const where = `outputs[${i}]`;
  if (!isMap(out)) { err(`${where} must be a mapping`); continue; }
  if (!isStr(out.label) || !out.label) {
    err(`${where}.label must be a non-empty string`);
  } else {
    if (outputLabels.has(out.label)) err(`${where}.label '${out.label}' is duplicated`);
    outputLabels.add(out.label);
  }
  if (!isStr(out.type)) err(`${where}.type must be a string`);
}

// ── sql/ folder index ───────────────────────────────────────────────────────
const sqlDir = path.join(profileDir, 'sql');
const sqlOnDisk = new Set();
function walk(dir) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) walk(full);
    else if (ent.isFile() && ent.name.endsWith('.sql')) {
      sqlOnDisk.add(path.relative(sqlDir, full).split(path.sep).join('/'));
    }
  }
}
if (fs.existsSync(sqlDir) && fs.statSync(sqlDir).isDirectory()) walk(sqlDir);
const referencedSql = new Set();

// ── normalise step input refs ───────────────────────────────────────────────
function stepInputLabels(refs, where) {
  const out = [];
  if (refs === undefined || refs === null) return out;
  if (!isList(refs)) { err(`${where} must be a list`); return out; }
  for (const [k, r] of refs.entries()) {
    if (isStr(r)) out.push(r);
    else if (isMap(r)) {
      if (!isStr(r.label) || !r.label) { err(`${where}[${k}].label must be a non-empty string`); continue; }
      if ('validate' in r && !isBool(r.validate)) err(`${where}[${k}].validate must be true or false`);
      out.push(r.label);
    } else err(`${where}[${k}] must be a string or mapping`);
  }
  return out;
}

// ── SQL placeholder check ───────────────────────────────────────────────────
const placeholderRe = /\{\{\s*(input|output)\s*:\s*([^}]+?)\s*\}\}/g;
const inputFileRe = /\{\{\s*input_file\s*\}\}/;

function stripSqlComments(sql) {
  // Strip /* ... */ block comments and -- line comments so placeholder
  // examples inside documentation don't get flagged.
  return sql
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/--[^\n]*/g, '');
}

function checkSqlPlaceholders(sqlPath, inpLabels, outLabels, where) {
  let raw;
  try { raw = fs.readFileSync(sqlPath, 'utf8'); }
  catch (e) { err(`${where}: cannot read sql file: ${e.message}`); return; }
  const sql = stripSqlComments(raw);
  const hasInputFile = inputFileRe.test(sql);
  const usedInputs = [], usedOutputs = [];
  for (const m of sql.matchAll(placeholderRe)) {
    (m[1] === 'input' ? usedInputs : usedOutputs).push(m[2].trim());
  }
  if (hasInputFile && inpLabels.length !== 1) {
    err(`${where}: uses {{input_file}} but transform declares ${inpLabels.length} inputs (must be 1)`);
  }
  if (!hasInputFile && usedInputs.length === 0 && inpLabels.length > 1) {
    err(`${where}: multi-input transform uses no {{input:Label}} placeholders`);
  }
  for (const u of usedInputs) {
    if (!inpLabels.includes(u)) err(`${where}: SQL references unknown input '${u}' (transform inputs: [${inpLabels.join(', ')}])`);
  }
  for (const u of usedOutputs) {
    if (!outLabels.includes(u)) err(`${where}: SQL references unknown output '${u}' (transform outputs: [${outLabels.join(', ')}])`);
  }
  if (outLabels.length > 1) {
    for (const o of outLabels) {
      if (!usedOutputs.includes(o)) err(`${where}: multi-output transform but SQL never writes to {{output:${o}}}`);
    }
  }
}

// ── transform check ─────────────────────────────────────────────────────────
function checkTransform(t, where) {
  const inpLabels = stepInputLabels(t.input, `${where}.input`);
  for (const lbl of inpLabels) {
    if (!inputLabels.has(lbl)) err(`${where}.input references unknown input label '${lbl}'`);
  }
  let sqlName = t.sql;
  if (!isStr(sqlName) || !sqlName) {
    err(`${where}.sql must be a non-empty string`);
    sqlName = null;
  } else {
    referencedSql.add(sqlName);
    if (!fs.existsSync(path.join(sqlDir, sqlName))) {
      err(`${where}.sql references missing file sql/${sqlName}`);
      sqlName = null;
    }
  }
  let outs = t.output;
  if (outs === undefined || outs === null) {
    err(`${where}.output must be a list of output labels`);
    outs = [];
  } else if (!isList(outs) || !outs.every(isStr)) {
    err(`${where}.output must be a list of strings`);
    outs = [];
  } else {
    for (const o of outs) if (!outputLabels.has(o)) err(`${where}.output references unknown output label '${o}'`);
  }
  if (sqlName !== null) {
    checkSqlPlaceholders(path.join(sqlDir, sqlName), inpLabels, outs, `${where} (sql/${sqlName})`);
  }
  if (t.notices !== undefined && t.notices !== null) {
    if (!isList(t.notices)) err(`${where}.notices must be a list`);
    else for (const [ni, n] of t.notices.entries()) {
      const nw = `${where}.notices[${ni}]`;
      if (!isMap(n)) { err(`${nw} must be a mapping`); continue; }
      if (!isStr(n.label)) err(`${nw}.label must be a string`);
      if (!isStr(n.sql)) err(`${nw}.sql must be a string`);
      else {
        referencedSql.add(n.sql);
        if (!fs.existsSync(path.join(sqlDir, n.sql))) err(`${nw}.sql references missing file sql/${n.sql}`);
      }
    }
  }
}

// ── steps ───────────────────────────────────────────────────────────────────
const stepLabels = [];
let needsSqlDir = false;
const steps = data.steps || [];
if (!isList(data.steps)) err('steps must be a list');

for (const [i, step] of steps.entries()) {
  let where = `steps[${i}]`;
  if (!isMap(step)) { err(`${where} must be a mapping`); continue; }
  if (!isStr(step.label) || !step.label) err(`${where}.label must be a non-empty string`);
  else { stepLabels.push(step.label); where = `steps[${i}](${step.label})`; }
  const t = step.type;
  if (t !== 'file_input' && t !== 'sql_transform' && t !== 'manual_instruction') {
    err(`${where}.type must be one of file_input, sql_transform, manual_instruction (got ${JSON.stringify(t)})`);
    continue;
  }
  if (t === 'file_input') {
    if (step.input === undefined || step.input === null) err(`${where} (file_input): input is required`);
    else for (const l of stepInputLabels(step.input, `${where}.input`)) {
      if (!inputLabels.has(l)) err(`${where}.input references unknown input label '${l}'`);
    }
  } else if (t === 'sql_transform') {
    needsSqlDir = true;
    if (step.transforms !== undefined && step.transforms !== null) {
      if (!isList(step.transforms) || step.transforms.length === 0) {
        err(`${where}.transforms must be a non-empty list`);
      } else for (const [ti, tr] of step.transforms.entries()) {
        if (!isMap(tr)) { err(`${where}.transforms[${ti}] must be a mapping`); continue; }
        checkTransform(tr, `${where}.transforms[${ti}]`);
      }
    } else {
      checkTransform({ input: step.input, sql: step.sql, output: step.output, notices: step.notices }, where);
    }
  }
}

if (needsSqlDir && (!fs.existsSync(sqlDir) || !fs.statSync(sqlDir).isDirectory())) {
  err('sql/ directory is missing but sql_transform steps are declared');
}
for (const unused of [...sqlOnDisk].filter(s => !referencedSql.has(s)).sort()) {
  info(`sql/${unused} is not referenced by any transform or notice`);
}

// ── instructions.md ─────────────────────────────────────────────────────────
const mdPath = path.join(profileDir, 'instructions.md');
if (fs.existsSync(mdPath)) {
  const md = fs.readFileSync(mdPath, 'utf8');
  const anchors = new Set();
  for (const raw of md.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith('<!-- label:') && line.endsWith('-->')) {
      const inner = line.slice('<!-- label:'.length, -'-->'.length).trim();
      if (inner) anchors.add(inner);
    }
  }
  for (const a of anchors) if (!stepLabels.includes(a)) err(`instructions.md has section for unknown step label '${a}'`);
  for (const s of stepLabels) if (!anchors.has(s)) info(`instructions.md has no section for step '${s}'`);
} else {
  info('instructions.md not present');
}

for (const m of infos) console.error(`  i ${m}`);
for (const m of errors) console.error(`  x ${m}`);
process.exit(errors.length ? 1 : 0);
JS
}

for profile_dir in "$SRC_DIR"/*/; do
  name="$(basename "$profile_dir")"
  out="$SCRIPT_DIR/$name.import"

  echo "Verifying $name..."
  if ! verify_profile "$profile_dir"; then
    echo "  x verification failed for $name -- skipping build" >&2
    exit 1
  fi

  echo "Building $name.import..."
  rm -f "$out"
  (cd "$profile_dir" && zip -r --quiet "$out" . \
      --exclude "test-files/*" \
      --exclude "*.DS_Store")
  echo "  -> $out"
done

echo "Done."
