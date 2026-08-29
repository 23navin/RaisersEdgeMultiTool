// workspace.rs
//
// Session-scoped working directories and the opaque artifact ids the frontend
// holds between calls. Generalizes the per-run token report.rs introduced:
// every profile load gets its own session directory under one root, every
// transform run gets its own subdirectory, and nothing is keyed on a name two
// concurrent users could share.
//
// Layout under `<root>/<session_id>/`:
//   profile/        the extracted .import bundle (structure.yaml, sql/, ...)
//   runs/<token>/   one dir per transform run — output CSVs land here
//   queries/        query_<label>.json from re_query steps
//   codetables/     codetable_<label>.json pulls + sync_<label>.json outcomes
//   inputs/         files registered/uploaded for this session
//
// An ArtifactId is a forward-slash relative path under the session dir (e.g.
// "runs/1a2b-0/import_file.csv"). The client treats it as opaque; `resolve`
// re-anchors it and rejects anything that would escape the session — that
// containment check is what makes client-supplied ids safe once this runs
// behind a server.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::errors::AppError;

// Distinct per call within a process. The counter alone would collide across
// processes (concurrent test binaries, a second app instance), and the clock
// alone can repeat under a coarse timer, so use both. (Moved from report.rs,
// which pioneered the pattern for report run dirs.)
pub fn unique_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", nanos, n)
}

// A session id is minted by `create` and echoed back by the client on every
// later call. Only the charset minted below is accepted on the way back in.
fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub struct Workspace {
    root: PathBuf,     // <workspaces_root>/<session_id>
    session_id: String,
}

impl Workspace {
    // Mint a fresh session under `workspaces_root`.
    pub fn create(workspaces_root: &Path) -> Result<Self, AppError> {
        let session_id = format!("s-{}", unique_token());
        let root = workspaces_root.join(&session_id);
        fs::create_dir_all(&root)
            .map_err(|e| AppError::IoError(format!("Cannot create session dir: {}", e)))?;
        Ok(Self { root, session_id })
    }

    // Open an existing session from a client-supplied id.
    pub fn open(workspaces_root: &Path, session_id: &str) -> Result<Self, AppError> {
        if !valid_session_id(session_id) {
            return Err(AppError::ParseError(format!(
                "Invalid session id: {:?}",
                session_id
            )));
        }
        let root = workspaces_root.join(session_id);
        if !root.is_dir() {
            return Err(AppError::IoError(format!(
                "Session '{}' not found — reload the profile and try again",
                session_id
            )));
        }
        Ok(Self {
            root,
            session_id: session_id.to_string(),
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    // The extracted profile bundle lives here; created by the loader.
    pub fn profile_dir(&self) -> PathBuf {
        self.root.join("profile")
    }

    // A fresh, unique directory for one transform/report run.
    pub fn new_run_dir(&self) -> Result<PathBuf, AppError> {
        let dir = self.root.join("runs").join(unique_token());
        fs::create_dir_all(&dir)
            .map_err(|e| AppError::IoError(format!("Cannot create run dir: {}", e)))?;
        Ok(dir)
    }

    pub fn queries_dir(&self) -> PathBuf {
        self.root.join("queries")
    }

    pub fn codetables_dir(&self) -> PathBuf {
        self.root.join("codetables")
    }

    pub fn inputs_dir(&self) -> PathBuf {
        self.root.join("inputs")
    }

    // Turn an absolute path inside this session into the id the client holds.
    pub fn artifact_id(&self, path: &Path) -> Result<String, AppError> {
        let rel = path.strip_prefix(&self.root).map_err(|_| {
            AppError::IoError(format!(
                "Path {} is outside the session workspace",
                path.display()
            ))
        })?;
        Ok(rel.to_string_lossy().replace('\\', "/"))
    }

    // Re-anchor a client-supplied artifact id, refusing anything that would
    // escape the session dir. Purely lexical — the id must be a plain relative
    // path with no `..`/`.` hops, no absolute form, no drive prefix.
    pub fn resolve(&self, artifact_id: &str) -> Result<PathBuf, AppError> {
        use std::path::Component;
        let bad = || {
            AppError::ParseError(format!("Invalid artifact id: {:?}", artifact_id))
        };
        if artifact_id.is_empty() || artifact_id.contains('\\') {
            return Err(bad());
        }
        let rel = Path::new(artifact_id);
        for c in rel.components() {
            match c {
                Component::Normal(_) => {}
                _ => return Err(bad()), // RootDir, Prefix, ParentDir, CurDir
            }
        }
        Ok(self.root.join(rel))
    }
}

// Delete sessions whose directory hasn't been touched in `max_age`. Best
// effort — a session mid-use has a fresh mtime, and one that can't be removed
// is skipped rather than failing the caller.
pub fn reap_stale(workspaces_root: &Path, max_age: Duration) -> usize {
    let Ok(entries) = fs::read_dir(workspaces_root) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut reaped = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .map(|age| age > max_age)
            .unwrap_or(false);
        if stale && fs::remove_dir_all(&path).is_ok() {
            reaped += 1;
        }
    }
    reaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-test-{}-{}", name, unique_token()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_open_roundtrip() {
        let root = temp_root("roundtrip");
        let ws = Workspace::create(&root).unwrap();
        let again = Workspace::open(&root, ws.session_id()).unwrap();
        assert_eq!(again.profile_dir(), ws.profile_dir());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn open_rejects_bad_ids() {
        let root = temp_root("bad-ids");
        assert!(Workspace::open(&root, "../escape").is_err());
        assert!(Workspace::open(&root, "").is_err());
        assert!(Workspace::open(&root, "no/slashes").is_err());
        assert!(Workspace::open(&root, "does-not-exist").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resolve_rejects_escapes() {
        let root = temp_root("resolve");
        let ws = Workspace::create(&root).unwrap();
        assert!(ws.resolve("runs/a/out.csv").is_ok());
        assert!(ws.resolve("../other-session/out.csv").is_err());
        assert!(ws.resolve("/etc/passwd").is_err());
        assert!(ws.resolve("runs/../../x").is_err());
        assert!(ws.resolve("").is_err());
        assert!(ws.resolve("runs\\a\\out.csv").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn artifact_id_relativizes() {
        let root = temp_root("artifact");
        let ws = Workspace::create(&root).unwrap();
        let run = ws.new_run_dir().unwrap();
        let id = ws.artifact_id(&run.join("out.csv")).unwrap();
        assert!(id.starts_with("runs/"));
        assert_eq!(ws.resolve(&id).unwrap(), run.join("out.csv"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reaper_removes_only_stale() {
        let root = temp_root("reap");
        let ws = Workspace::create(&root).unwrap();
        // Fresh session survives a 1-hour threshold.
        assert_eq!(reap_stale(&root, Duration::from_secs(3600)), 0);
        assert!(Workspace::open(&root, ws.session_id()).is_ok());
        // Zero threshold reaps it.
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(reap_stale(&root, Duration::from_secs(0)), 1);
        assert!(Workspace::open(&root, ws.session_id()).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
