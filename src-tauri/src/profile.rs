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
use std::io::Read;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::errors::AppError;

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
    // Open the zip file
    let file = fs::File::open(zip_path)
        .map_err(|e| AppError::IoError(format!("Cannot open profile zip: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::IoError(format!("Cannot read zip archive: {}", e)))?;

    // Extract to a temp directory named after the zip file stem
    // e.g. profiles/vendor_a.import → /tmp/import-tool-vendor_a/
    let stem = zip_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let temp_dir = std::env::temp_dir().join(format!("import-tool-{}", stem));
    fs::create_dir_all(&temp_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create temp dir: {}", e)))?;

    // Extract every file in the zip
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

    // Read all .sql files from the sql/ folder
    let mut sql_files: HashMap<String, String> = HashMap::new();
    let sql_dir = dir.join("sql");
    if sql_dir.exists() {
        for entry in fs::read_dir(&sql_dir)
            .map_err(|e| AppError::IoError(e.to_string()))?
        {
            let entry = entry.map_err(|e| AppError::IoError(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("sql") {
                let filename = path.file_name().unwrap().to_string_lossy().to_string();
                let content = fs::read_to_string(&path)
                    .map_err(|e| AppError::IoError(format!("Cannot read {}: {}", filename, e)))?;
                sql_files.insert(filename, content);
            }
        }
    }

    Ok(LoadedProfile {
        structure,
        instructions,
        sql_files,
        temp_dir: dir.to_path_buf(),
    })
}

// ── Profile listing ───────────────────────────────────────────────────────────
// Scans the profiles/ folder for .zip files.
// Returns lightweight metadata for each — just enough to populate the dropdown.
// Does NOT fully load each profile (that happens when the user selects one).

#[derive(Debug, Serialize, Clone)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub zip_path: String,   // full path — passed back to load_profile later
}

pub fn list_profiles(profiles_dir: &Path) -> Result<Vec<ProfileSummary>, AppError> {
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

        let mut yaml_entry = archive.by_name("structure.yaml")
            .map_err(|_| AppError::ParseError(
                format!("{} is missing structure.yaml", path.display())
            ))?;

        let mut yaml_content = String::new();
        yaml_entry.read_to_string(&mut yaml_content)
            .map_err(|e| AppError::IoError(e.to_string()))?;

        let structure: ProfileStructure = serde_yaml::from_str(&yaml_content)
            .map_err(|e| AppError::ParseError(format!("Invalid structure.yaml: {}", e)))?;

        profiles.push(ProfileSummary {
            id: structure.id,
            name: structure.name,
            version: structure.version,
            zip_path: path.to_string_lossy().to_string(),
        });
    }

    Ok(profiles)
}