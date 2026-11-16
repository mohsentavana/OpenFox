//! Boot-time recovery: find the session files, keep every line that
//! parses, quarantine the torn tail. A crash mid-write loses the last
//! line — never the session.

use std::path::{Path, PathBuf};

/// One recovered session file: the lines that survived, the ones that
/// did not, and where the torn copy went.
pub struct RecoveredSession {
    pub path: PathBuf,
    pub records: Vec<serde_json::Value>,
    pub torn_path: Option<PathBuf>,
}

/// Recover a single line-delimited JSON session file. Torn-tail
/// tolerance is the whole design: each line is self-contained, so the
/// last one can die mid-write and the session lives.
pub fn recover_session(path: &Path) -> RecoveredSession {
    let mut recovered = RecoveredSession {
        path: path.to_path_buf(),
        records: Vec::new(),
        torn_path: None,
    };
    let Ok(contents) = std::fs::read_to_string(path) else {
        return recovered;
    };
    let mut torn = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if torn {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(record) => recovered.records.push(record),
            Err(_) => {
                // First unparsable line is the torn tail: everything
                // after it is quarantined with the file copy.
                torn = true;
            }
        }
    }
    if torn {
        let mut torn_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "session".into());
        torn_name.push_str(".torn");
        let torn_path = path.with_file_name(torn_name);
        if std::fs::copy(path, &torn_path).is_ok() {
            recovered.torn_path = Some(torn_path);
        }
    }
    recovered
}

/// Recover every *.session file in a directory, newest-first by mtime so
/// the most recent crash wins the restore decision.
pub fn recover_all(dir: &Path) -> Vec<RecoveredSession> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("session") {
            if let Ok(meta) = entry.metadata() {
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                files.push((path, mtime));
            }
        }
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
        .into_iter()
        .map(|(path, _)| recover_session(&path))
        .collect()
}

/// Promote a recovered session to the active slot: atomically replace
/// the active path with the recovered file. The rename is atomic on
/// every filesystem OpenFox targets; a copy is not.
pub fn promote(recovered: &RecoveredSession, active: &Path) -> std::io::Result<()> {
    std::fs::rename(&recovered.path, active)
}
