//! Presets: named settings, saved in `presets.txt` next to the program
//! (the program folder can be copied together with the presets)

use std::path::PathBuf;

fn presets_file() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("presets.txt"))
}

/// Reads the saved presets: one per line, "name<TAB>settings"
pub(crate) fn load_presets() -> Vec<(String, String)> {
    let Some(text) = presets_file().and_then(|file| std::fs::read_to_string(file).ok()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, settings)| (name.to_string(), settings.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// Writes all presets to the file
pub(crate) fn save_presets(presets: &[(String, String)]) {
    let Some(file) = presets_file() else { return };
    let text: String = presets
        .iter()
        .map(|(name, settings)| format!("{}\t{}\n", clean_name(name), settings))
        .collect();
    if let Err(e) = std::fs::write(&file, text) {
        // E.g. if the program is in a folder without write access ("Program Files")
        eprintln!("Could not save presets to '{}': {}", file.to_string_lossy(), e);
    }
}

/// Preset names may not contain tabs or line breaks (they separate the entries in the file)
pub(crate) fn clean_name(name: &str) -> String {
    name.replace(['\t', '\n', '\r'], " ").trim().to_string()
}
