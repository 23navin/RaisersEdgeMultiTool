// profile.rs
//
// Owns everything related to profile bundles:
//   - Unzipping the bundle into a temp directory
//   - Parsing structure.yaml into typed Rust structs
//   - Reading instructions.md
//   - Parsing instructions.md into per-step sections
//   - Listing available profiles from the profiles/ folder

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::errors::AppError;

// ── Built-in profiles ────────────────────────────────────────────────────────
// Embedded at compile time so the binary ships with a usable set of profiles
// regardless of what's on disk. Add/remove a line per builtin. The .import
// files must exist at build time (run profiles/build.sh).

const BUILTIN_PROFILES: &[(&str, &[u8])] = &[
    ("test1.import", include_bytes!("../../profiles/test1.import")),
    ("test2.import", include_bytes!("../../profiles/test2.import")),
    ("test3.import", include_bytes!("../../profiles/test3.import")),
    ("test4.import", include_bytes!("../../profiles/test4.import")),
];

// ── YAML structs ──────────────────────────────────────────────────────────────
// These mirror the shape of structure.yaml exactly.
// serde_yaml deserializes the file directly into these structs.
// Every field name must match the YAML key, or use #[serde(rename = "...")]

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColumnValidation {
    pub label: String,
    pub required: bool,
    #[serde(rename = "type")]
    pub col_type: String,        // "string", "number" etc — 'type' is reserved in Rust
    pub digits: Option<u32>,     // only for number columns
    pub value: Option<Vec<String>>, // allowed values, if restricted
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InputDefinition {
    pub label: String,
    #[serde(rename = "type")]
    pub input_type: String,      // "csv", "xlsx"
    pub required: bool,
    pub validation: Option<Vec<ColumnValidation>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OutputDefinition {
    pub label: String,
    #[serde(rename = "type")]
    pub output_type: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StepInput {
    pub label: String,
    pub validate: Option<bool>
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
// StepInputRef handles both forms that appear in structure.yaml:
//   - Short form:  "Classification"  (just a string)
//   - Long form:   { label: Classification, validate: true }
pub enum StepInputRef {
    Simple(String),
    Detailed(StepInput),
}

// A notice query attached to a sql_transform. Runs after the main transform
// succeeds; returned rows become informational items the user needs to
// address externally (e.g. "this category isn't in the master list").
// SQL filename resolves against the bundle's sql/ folder.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct NoticeQuery {
    pub label: String,
    pub sql: String,
    pub description: Option<String>,
}

// One transform unit inside a sql_transform step. Use the step-level
// input/sql/output for a single transform; use `transforms` for multiple.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SqlTransform {
    pub input: Option<Vec<StepInputRef>>,
    pub sql: String,
    pub output: Option<Vec<String>>,
    pub notices: Option<Vec<NoticeQuery>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Step {
    pub label: String,
    #[serde(rename = "type")]
    pub step_type: String,       // "file_input", "sql_transform", "manual_instruction"
    pub input: Option<Vec<StepInputRef>>,
    pub sql: Option<String>,     // sql_transform single-transform shortcut
    pub output: Option<Vec<String>>, // sql_transform single-transform shortcut
    pub notices: Option<Vec<NoticeQuery>>, // sql_transform single-transform shortcut
    pub transforms: Option<Vec<SqlTransform>>, // sql_transform multi-transform form
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProfileStructure {
    pub id: String,
    pub name: String,
    pub version: String,
    pub min_app_version: String,
    pub inputs: Vec<InputDefinition>,
    pub outputs: Vec<OutputDefinition>,
    pub steps: Vec<Step>,
}

// ── Parsed profile (what the rest of the app works with) ─────────────────────
// ProfileStructure is the raw YAML shape.
// LoadedProfile is what gets passed around after parsing — includes
// the extracted SQL content and parsed instruction sections.

#[derive(Debug, Clone, Serialize)]
pub struct LoadedProfile {
    pub structure: ProfileStructure,
    pub instructions: HashMap<String, String>, // step label → markdown content
    pub sql_files: HashMap<String, String>,    // filename → SQL content
    pub temp_dir: PathBuf,                     // where the zip was extracted
    pub files: Vec<ProfileFileEntry>,          // raw editable text files (used by the profile editor)
}

// ── Instruction parser ────────────────────────────────────────────────────────
// Splits instructions.md into sections keyed by step label.
// Looks for HTML comment anchors: <!-- label: StepLabel -->
// Everything between one anchor and the next belongs to that step.
// Content before the first anchor is stored under "_header".

pub fn parse_instructions(md_content: &str) -> HashMap<String, String> {
    let mut sections: HashMap<String, String> = HashMap::new();
    let mut current_label = "_header".to_string();
    let mut current_content = String::new();

    for line in md_content.lines() {
        let trimmed = line.trim();

        // Check if this line is a label anchor comment
        if trimmed.starts_with("<!-- label:") && trimmed.ends_with("-->") {
            // Save whatever we've accumulated under the previous label
            if !current_content.trim().is_empty() {
                sections.insert(current_label.clone(), current_content.trim().to_string());
            }
            // Extract the label name from <!-- label: LabelName -->
            let inner = &trimmed["<!-- label:".len()..trimmed.len() - "-->".len()];
            current_label = inner.trim().to_string();
            current_content = String::new();
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Don't forget the last section
    if !current_content.trim().is_empty() {
        sections.insert(current_label, current_content.trim().to_string());
    }

    sections
}

// ── Bundle loader ─────────────────────────────────────────────────────────────
// Unzips a profile bundle into a system temp directory.
// Reads structure.yaml, instructions.md, and all .sql files.
// Returns a LoadedProfile ready for the rest of the app to use.

pub fn load_profile(zip_path: &Path) -> Result<LoadedProfile, AppError> {
    let file = fs::File::open(zip_path)
        .map_err(|e| AppError::IoError(format!("Cannot open profile zip: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::IoError(format!("Cannot read zip archive: {}", e)))?;

    let stem = zip_path.file_stem().unwrap_or_default().to_string_lossy();
    let temp_dir = extract_to_clean_temp_dir(&mut archive, &stem)?;
    load_from_dir(&temp_dir)
}

// Extract to /tmp/import-tool-<stem>/, wiping any prior contents first so
// stale entries from earlier extractions (different bundle, Finder-style zip
// with __MACOSX/ siblings, etc.) don't leak into the loaded profile.
fn extract_to_clean_temp_dir<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    stem: &str,
) -> Result<PathBuf, AppError> {
    let temp_dir = std::env::temp_dir().join(format!("import-tool-{}", stem));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| AppError::IoError(format!("Cannot clear temp dir: {}", e)))?;
    }
    fs::create_dir_all(&temp_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create temp dir: {}", e)))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| AppError::IoError(e.to_string()))?;
        let out_path = temp_dir.join(entry.name());
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| AppError::IoError(e.to_string()))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| AppError::IoError(e.to_string()))?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| AppError::IoError(e.to_string()))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| AppError::IoError(e.to_string()))?;
        }
    }
    Ok(temp_dir)
}

// ── load_builtin ──────────────────────────────────────────────────────────────
// Mirrors load_profile but reads the zip from embedded bytes instead of disk.
// `name` is the BUILTIN_PROFILES key (e.g. "test1.import").

pub fn load_builtin(name: &str) -> Result<LoadedProfile, AppError> {
    let bytes = BUILTIN_PROFILES.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
        .ok_or_else(|| AppError::ParseError(format!("Unknown builtin profile: {}", name)))?;

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::IoError(format!("Cannot read builtin zip: {}", e)))?;

    let stem = Path::new(name).file_stem().unwrap_or_default().to_string_lossy();
    let temp_dir = extract_to_clean_temp_dir(&mut archive, &stem)?;
    load_from_dir(&temp_dir)
}

// ── load_from_dir ─────────────────────────────────────────────────────────────
// Reads a profile from an already-extracted directory.
// Called by validate_file and run_profile, which receive temp_dir from the
// frontend rather than the original zip path.

pub fn load_from_dir(dir: &Path) -> Result<LoadedProfile, AppError> {
    // Read and parse structure.yaml
    let yaml_path = dir.join("structure.yaml");
    let yaml_content = fs::read_to_string(&yaml_path)
        .map_err(|e| AppError::IoError(format!("Cannot read structure.yaml: {}", e)))?;
    let structure: ProfileStructure = serde_yaml::from_str(&yaml_content)
        .map_err(|e| AppError::ParseError(format!("Invalid structure.yaml: {}", e)))?;

    // Read and parse instructions.md (optional)
    let md_path = dir.join("instructions.md");
    let instructions = if md_path.exists() {
        let md_content = fs::read_to_string(&md_path)
            .map_err(|e| AppError::IoError(format!("Cannot read instructions.md: {}", e)))?;
        parse_instructions(&md_content)
    } else {
        HashMap::new()
    };

    // Read all .sql files from the sql/ folder. Walks subdirectories so
    // notice queries under sql/notices/ are picked up. Keys are paths
    // relative to sql_dir with forward slashes, matching how YAML
    // references them (e.g. "notices/unknown_categories.sql").
    let mut sql_files: HashMap<String, String> = HashMap::new();
    let sql_dir = dir.join("sql");
    if sql_dir.exists() {
        collect_sql_files(&sql_dir, &sql_dir, &mut sql_files)?;
    }

    // Snapshot every editable text file in the bundle for the profile editor.
    // Limited to extensions we know are text — keeps binary assets (images
    // referenced from instructions.md) out of the JSON payload.
    let mut files: Vec<ProfileFileEntry> = Vec::new();
    collect_text_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(LoadedProfile {
        structure,
        instructions,
        sql_files,
        temp_dir: dir.to_path_buf(),
        files,
    })
}

// macOS AppleDouble files (`__MACOSX/` directories, `._*` filenames) carry
// extended-attribute metadata, not file content. They share extensions with
// the files they describe (so `._instructions.md` passes a .md extension
// check) but contain binary data that fails UTF-8 decode.
fn is_macos_metadata(name: &str) -> bool {
    name == "__MACOSX" || name.starts_with("._")
}

fn collect_text_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<ProfileFileEntry>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(dir).map_err(|e| AppError::IoError(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::IoError(e.to_string()))?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_macos_metadata(name) {
            continue;
        }
        if path.is_dir() {
            collect_text_files(root, &path, out)?;
            continue;
        }
        let is_text = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml") | Some("md") | Some("sql"),
        );
        if !is_text {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| AppError::IoError(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let content = fs::read_to_string(&path)
            .map_err(|e| AppError::IoError(format!("Cannot read {}: {}", rel, e)))?;
        out.push(ProfileFileEntry { path: rel, content });
    }
    Ok(())
}

fn collect_sql_files(
    root: &Path,
    dir: &Path,
    out: &mut HashMap<String, String>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(dir).map_err(|e| AppError::IoError(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::IoError(e.to_string()))?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_macos_metadata(name) {
            continue;
        }
        if path.is_dir() {
            collect_sql_files(root, &path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| AppError::IoError(e.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let content = fs::read_to_string(&path)
                .map_err(|e| AppError::IoError(format!("Cannot read {}: {}", rel, e)))?;
            out.insert(rel, content);
        }
    }
    Ok(())
}

// ── Profile listing ───────────────────────────────────────────────────────────
// Scans the profiles/ folder for .zip files.
// Returns lightweight metadata for each — just enough to populate the dropdown.
// Does NOT fully load each profile (that happens when the user selects one).

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ProfileSource {
    Builtin,
    User,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub zip_path: String,   // user: full fs path. builtin: "builtin://<filename>" sentinel.
    pub source: ProfileSource,
}

// Peeks at structure.yaml inside an open zip archive — enough to populate
// one ProfileSummary without extracting the whole bundle.
fn read_summary_from_zip<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    zip_path: String,
    source: ProfileSource,
    display: &str,
) -> Result<ProfileSummary, AppError> {
    let mut yaml_entry = archive.by_name("structure.yaml")
        .map_err(|_| AppError::ParseError(
            format!("{} is missing structure.yaml", display)
        ))?;

    let mut yaml_content = String::new();
    yaml_entry.read_to_string(&mut yaml_content)
        .map_err(|e| AppError::IoError(e.to_string()))?;

    let structure: ProfileStructure = serde_yaml::from_str(&yaml_content)
        .map_err(|e| AppError::ParseError(format!("Invalid structure.yaml: {}", e)))?;

    Ok(ProfileSummary {
        id: structure.id,
        name: structure.name,
        version: structure.version,
        zip_path,
        source,
    })
}

pub fn list_builtin_profiles() -> Result<Vec<ProfileSummary>, AppError> {
    let mut profiles = Vec::new();
    for (name, bytes) in BUILTIN_PROFILES {
        let mut archive = zip::ZipArchive::new(Cursor::new(*bytes))
            .map_err(|e| AppError::IoError(format!("Cannot read builtin {}: {}", name, e)))?;
        let summary = read_summary_from_zip(
            &mut archive,
            format!("builtin://{}", name),
            ProfileSource::Builtin,
            name,
        )?;
        profiles.push(summary);
    }
    profiles.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(profiles)
}

pub fn list_user_profiles(profiles_dir: &Path) -> Result<Vec<ProfileSummary>, AppError> {
    let mut profiles = Vec::new();

    if !profiles_dir.exists() {
        return Ok(profiles); // empty list, not an error
    }

    for entry in fs::read_dir(profiles_dir)
        .map_err(|e| AppError::IoError(format!("Cannot read profiles dir: {}", e)))?
    {
        let entry = entry.map_err(|e| AppError::IoError(e.to_string()))?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("import") {
            continue; // skip non-zip files
        }

        // Peek inside the zip just enough to read structure.yaml
        // without fully extracting — keeps listing fast
        let file = fs::File::open(&path)
            .map_err(|e| AppError::IoError(e.to_string()))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::IoError(e.to_string()))?;

        let summary = read_summary_from_zip(
            &mut archive,
            path.to_string_lossy().to_string(),
            ProfileSource::User,
            &path.display().to_string(),
        )?;
        profiles.push(summary);
    }

    profiles.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(profiles)
}

// ── Editor write path ─────────────────────────────────────────────────────────
// The settings panel needs to read every editable file in a bundle, save edits
// back, create new bundles, duplicate existing ones, and delete user bundles.
// Built-in bundles are read-only; mutating commands refuse a `builtin://` path.

// One editable file inside a profile bundle. Path is relative to the bundle
// root (e.g. "structure.yaml", "sql/primary.sql"). Forward-slash separators
// only — the zip writer treats `\` as a literal filename character.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProfileFileEntry {
    pub path: String,
    pub content: String,
}

const BUILTIN_PREFIX: &str = "builtin://";

fn ensure_user_zip(zip_path: &str) -> Result<&Path, AppError> {
    if zip_path.starts_with(BUILTIN_PREFIX) {
        return Err(AppError::ParseError(
            "Built-in profiles are read-only. Duplicate first to edit.".into(),
        ));
    }
    Ok(Path::new(zip_path))
}

// Write a fresh .import zip containing only `files` — no binary preservation.
// Used by `create_new_profile` where there's no prior bundle to merge against.
fn write_import_zip(zip_path: &Path, files: &[ProfileFileEntry]) -> Result<(), AppError> {
    write_zip(zip_path, |writer, opts| {
        for f in files {
            let path = f.path.replace('\\', "/");
            writer.start_file(&path, *opts)
                .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
            writer.write_all(f.content.as_bytes())
                .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
        }
        Ok(())
    })
}

// Re-zip a source bundle, swapping in edited text files but preserving binary
// assets (images, etc.) at the byte level. New text files not present in the
// source are appended. Used by save_user_profile and duplicate_profile.
//
// `source_bytes` is the raw bytes of the source .import; for built-ins this
// is the in-memory BUILTIN_PROFILES slice, for user profiles it's the on-disk
// .import read into memory (so the source file handle is closed before we
// rename over its location).
fn rewrite_import_zip(
    source_bytes: &[u8],
    dest_path: &Path,
    text_overrides: &[ProfileFileEntry],
) -> Result<(), AppError> {
    use std::collections::HashSet;

    let overrides: HashMap<String, &str> = text_overrides
        .iter()
        .map(|f| (f.path.replace('\\', "/"), f.content.as_str()))
        .collect();

    write_zip(dest_path, |writer, opts| {
        let mut archive = zip::ZipArchive::new(Cursor::new(source_bytes))
            .map_err(|e| AppError::IoError(format!("Cannot read source zip: {}", e)))?;

        let mut written: HashSet<String> = HashSet::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)
                .map_err(|e| AppError::IoError(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            writer.start_file(&name, *opts)
                .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
            match overrides.get(&name) {
                Some(text) => writer.write_all(text.as_bytes())
                    .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?,
                None => {
                    // Binary-safe verbatim copy.
                    std::io::copy(&mut entry, writer)
                        .map(|_| ())
                        .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
                }
            }
            written.insert(name);
        }

        // Append overrides that weren't already in the source (e.g. files
        // created by the scaffold action or by a brand-new sql/ filename).
        for (path, content) in &overrides {
            if !written.contains(path) {
                writer.start_file(path, *opts)
                    .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
                writer.write_all(content.as_bytes())
                    .map_err(|e| AppError::IoError(format!("Zip write failed: {}", e)))?;
            }
        }

        Ok(())
    })
}

// Shared zip-write scaffolding: parent dir, tmp file, finalise, atomic rename.
fn write_zip<F>(dest_path: &Path, body: F) -> Result<(), AppError>
where
    F: FnOnce(
        &mut zip::ZipWriter<fs::File>,
        &zip::write::FileOptions<()>,
    ) -> Result<(), AppError>,
{
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::IoError(format!("Cannot create profiles dir: {}", e)))?;
    }
    let tmp_path = dest_path.with_extension("import.tmp");
    {
        let tmp_file = fs::File::create(&tmp_path)
            .map_err(|e| AppError::IoError(format!("Cannot create temp zip: {}", e)))?;
        let mut writer = zip::ZipWriter::new(tmp_file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        body(&mut writer, &opts)?;
        writer.finish()
            .map_err(|e| AppError::IoError(format!("Zip finalise failed: {}", e)))?;
    }
    fs::rename(&tmp_path, dest_path)
        .map_err(|e| AppError::IoError(format!("Cannot replace .import: {}", e)))?;
    Ok(())
}

// Resolve a source zip's raw bytes — either an embedded built-in payload or
// the on-disk .import file. The on-disk read happens up front so the source
// file handle is closed before any rename targets that path.
fn read_source_zip_bytes(source_zip_path: &str) -> Result<Vec<u8>, AppError> {
    if let Some(name) = source_zip_path.strip_prefix(BUILTIN_PREFIX) {
        let bytes = BUILTIN_PROFILES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| (*b).to_vec())
            .ok_or_else(|| AppError::ParseError(format!("Unknown builtin profile: {}", name)))?;
        Ok(bytes)
    } else {
        fs::read(source_zip_path)
            .map_err(|e| AppError::IoError(format!("Cannot read source profile: {}", e)))
    }
}

// Re-extract the saved bundle into its temp dir so the in-memory LoadedProfile
// the frontend already holds keeps matching what's on disk. Drops stale files
// that no longer exist in the new bundle.
fn refresh_temp_dir(zip_path: &Path) -> Result<PathBuf, AppError> {
    let stem = zip_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let temp_dir = std::env::temp_dir().join(format!("import-tool-{}", stem));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| AppError::IoError(format!("Cannot clear temp dir: {}", e)))?;
    }
    fs::create_dir_all(&temp_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create temp dir: {}", e)))?;

    let file = fs::File::open(zip_path)
        .map_err(|e| AppError::IoError(format!("Cannot reopen .import: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::IoError(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| AppError::IoError(e.to_string()))?;
        let out_path = temp_dir.join(entry.name());
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| AppError::IoError(e.to_string()))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| AppError::IoError(e.to_string()))?;
            }
            let mut out_file = fs::File::create(&out_path)
                .map_err(|e| AppError::IoError(e.to_string()))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| AppError::IoError(e.to_string()))?;
        }
    }
    Ok(temp_dir)
}

// Save edits to an existing user .import. Reads the new id/name/version
// from the supplied structure.yaml so the returned ProfileSummary reflects
// what's actually inside the bundle. Binary assets in the existing .import
// (e.g. images referenced from instructions.md) are preserved verbatim.
pub fn save_user_profile(
    zip_path: &str,
    files: &[ProfileFileEntry],
) -> Result<(ProfileSummary, LoadedProfile), AppError> {
    let path = ensure_user_zip(zip_path)?;

    let yaml = files
        .iter()
        .find(|f| f.path == "structure.yaml")
        .ok_or_else(|| AppError::ParseError("structure.yaml is required".into()))?;
    let structure: ProfileStructure = serde_yaml::from_str(&yaml.content)
        .map_err(|e| AppError::ParseError(format!("Invalid structure.yaml: {}", e)))?;

    // Read the existing bundle into memory before overwriting it. If the
    // .import doesn't exist yet (shouldn't normally happen for save_, but
    // belt-and-braces), fall back to a text-only write.
    if path.exists() {
        let source_bytes = fs::read(path)
            .map_err(|e| AppError::IoError(format!("Cannot read existing .import: {}", e)))?;
        rewrite_import_zip(&source_bytes, path, files)?;
    } else {
        write_import_zip(path, files)?;
    }
    let temp_dir = refresh_temp_dir(path)?;
    let loaded = load_from_dir(&temp_dir)?;

    let summary = ProfileSummary {
        id: structure.id,
        name: structure.name,
        version: structure.version,
        zip_path: path.to_string_lossy().to_string(),
        source: ProfileSource::User,
    };
    Ok((summary, loaded))
}

// Pick a filename inside `dir` that doesn't already exist on disk.
// Strategy: start with `{stem}.import`, then `{stem}-2.import`, etc.
fn pick_unique_path(dir: &Path, stem: &str) -> PathBuf {
    let safe: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stem = if safe.is_empty() { "profile".to_string() } else { safe };
    let mut candidate = dir.join(format!("{}.import", stem));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{}-{}.import", stem, n));
        n += 1;
    }
    candidate
}

// Create a fresh user profile with minimal structure.yaml and instructions.md.
// `profiles_dir` is the per-user profiles directory (under app_data_dir).
pub fn create_new_profile(profiles_dir: &Path) -> Result<(ProfileSummary, LoadedProfile), AppError> {
    fs::create_dir_all(profiles_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create profiles dir: {}", e)))?;

    // Find a unique id by scanning existing .import bundles.
    let mut existing_ids = Vec::new();
    if let Ok(read) = fs::read_dir(profiles_dir) {
        for entry in read.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("import") {
                continue;
            }
            if let Ok(file) = fs::File::open(entry.path()) {
                if let Ok(mut archive) = zip::ZipArchive::new(file) {
                    if let Ok(mut yaml) = archive.by_name("structure.yaml") {
                        let mut s = String::new();
                        if yaml.read_to_string(&mut s).is_ok() {
                            if let Ok(parsed) = serde_yaml::from_str::<ProfileStructure>(&s) {
                                existing_ids.push(parsed.id);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut id = "new_profile".to_string();
    let mut n = 2;
    while existing_ids.iter().any(|e| e == &id) {
        id = format!("new_profile_{}", n);
        n += 1;
    }

    let zip_path = pick_unique_path(profiles_dir, &id);
    let yaml = format!(
        "id: {}\nname: \"New Profile\"\nversion: \"0.1\"\nmin_app_version: \"0.1.0\"\ninputs: []\noutputs: []\nsteps: []\n",
        id,
    );
    let md = "# New Profile\n\nDescribe the import steps here.\n".to_string();
    let files = vec![
        ProfileFileEntry { path: "structure.yaml".into(), content: yaml },
        ProfileFileEntry { path: "instructions.md".into(), content: md },
    ];

    write_import_zip(&zip_path, &files)?;
    let temp_dir = refresh_temp_dir(&zip_path)?;
    let loaded = load_from_dir(&temp_dir)?;

    let summary = ProfileSummary {
        id: loaded.structure.id.clone(),
        name: loaded.structure.name.clone(),
        version: loaded.structure.version.clone(),
        zip_path: zip_path.to_string_lossy().to_string(),
        source: ProfileSource::User,
    };
    Ok((summary, loaded))
}

// Duplicate any profile (built-in or user) into the user profiles dir under
// a deduped id. Used by the editor when the user opens a built-in. Binary
// assets in the source bundle (e.g. images under assets/) are copied verbatim;
// only structure.yaml is patched to carry the new id/name.
pub fn duplicate_profile(
    source_zip_path: &str,
    profiles_dir: &Path,
) -> Result<(ProfileSummary, LoadedProfile), AppError> {
    // Read the source bundle's raw bytes; we'll re-zip from this without ever
    // calling fs::read_to_string on binary entries.
    let source_bytes = read_source_zip_bytes(source_zip_path)?;

    // Parse structure.yaml from the source so we can derive the new id/name.
    let original_yaml = read_yaml_from_zip_bytes(&source_bytes)?;
    let original_structure: ProfileStructure = serde_yaml::from_str(&original_yaml)
        .map_err(|e| AppError::ParseError(format!("Invalid source structure.yaml: {}", e)))?;

    // Pick a new id that doesn't collide with anything already on disk.
    let mut existing_ids = Vec::new();
    if let Ok(read) = fs::read_dir(profiles_dir) {
        for entry in read.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("import") {
                continue;
            }
            if let Ok(file) = fs::File::open(entry.path()) {
                if let Ok(mut archive) = zip::ZipArchive::new(file) {
                    if let Ok(mut yaml) = archive.by_name("structure.yaml") {
                        let mut s = String::new();
                        if yaml.read_to_string(&mut s).is_ok() {
                            if let Ok(parsed) = serde_yaml::from_str::<ProfileStructure>(&s) {
                                existing_ids.push(parsed.id);
                            }
                        }
                    }
                }
            }
        }
    }
    let base_id = format!("{}_copy", original_structure.id);
    let mut new_id = base_id.clone();
    let mut n = 2;
    while existing_ids.iter().any(|e| e == &new_id) {
        new_id = format!("{}_{}", base_id, n);
        n += 1;
    }
    let new_name = format!("{} (copy)", original_structure.name);

    // Patch structure.yaml's top-level id/name fields by surgical regex on
    // the raw YAML text — preserves comments, ordering, and quoting style.
    let patched_yaml = patch_yaml_id_name(&original_yaml, &new_id, &new_name);
    let overrides = vec![ProfileFileEntry {
        path: "structure.yaml".to_string(),
        content: patched_yaml,
    }];

    let zip_path = pick_unique_path(profiles_dir, &new_id);
    rewrite_import_zip(&source_bytes, &zip_path, &overrides)?;
    let temp_dir = refresh_temp_dir(&zip_path)?;
    let loaded = load_from_dir(&temp_dir)?;

    let summary = ProfileSummary {
        id: loaded.structure.id.clone(),
        name: loaded.structure.name.clone(),
        version: loaded.structure.version.clone(),
        zip_path: zip_path.to_string_lossy().to_string(),
        source: ProfileSource::User,
    };
    Ok((summary, loaded))
}

// Read structure.yaml out of a zip's raw bytes — used during duplicate so we
// can patch id/name without first extracting the whole bundle to disk.
fn read_yaml_from_zip_bytes(bytes: &[u8]) -> Result<String, AppError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::IoError(format!("Cannot read source zip: {}", e)))?;
    let mut entry = archive
        .by_name("structure.yaml")
        .map_err(|_| AppError::ParseError("Source bundle missing structure.yaml".into()))?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| AppError::IoError(format!("Cannot read source structure.yaml: {}", e)))?;
    Ok(buf)
}

// Replace the top-level `id:` and `name:` values in a structure.yaml string
// without touching the rest. Conservative regex: only matches when the key
// appears at column 0 — won't accidentally rewrite nested `name:` keys
// (e.g. inside `inputs:` definitions). Falls back to the original line if
// the pattern doesn't match either field.
fn patch_yaml_id_name(yaml: &str, new_id: &str, new_name: &str) -> String {
    let mut out = String::with_capacity(yaml.len());
    let mut id_done = false;
    let mut name_done = false;
    for line in yaml.lines() {
        if !id_done && line.starts_with("id:") {
            out.push_str(&format!("id: {}\n", new_id));
            id_done = true;
        } else if !name_done && line.starts_with("name:") {
            out.push_str(&format!("name: \"{}\"\n", new_name.replace('"', "\\\"")));
            name_done = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

// Remove a user profile from disk. Refuses built-ins.
pub fn delete_user_profile(zip_path: &str) -> Result<(), AppError> {
    let path = ensure_user_zip(zip_path)?;
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path)
        .map_err(|e| AppError::IoError(format!("Cannot delete profile: {}", e)))?;
    Ok(())
}