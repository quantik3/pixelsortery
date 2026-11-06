//! Presets: named settings, saved in a text file

use std::path::PathBuf;

/// `%APPDATA%\pixelsortery\presets.txt` on Windows, `~/.config/pixelsortery/presets.txt` elsewhere
fn presets_file() -> Option<PathBuf> {
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(dir.join("pixelsortery").join("presets.txt"))
}

/// Reads the saved presets: one per line, "name<TAB>settings"
pub(crate) fn load_presets() -> Vec<(String, String)> {
    let Some(file) = presets_file() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, settings)| (name.to_string(), settings.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// Writes all presets to the file
pub(crate) fn save_presets(presets: &[(String, String)]) {
    let Some(file) = presets_file() else { return };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text: String = presets
        .iter()
        .map(|(name, settings)| format!("{}\t{}\n", clean_name(name), settings))
        .collect();
    if let Err(e) = std::fs::write(&file, text) {
        log::warn!("Could not save presets to '{}': {}", file.to_string_lossy(), e);
    }
}

/// Preset names may not contain tabs or line breaks (they separate the entries in the file)
pub(crate) fn clean_name(name: &str) -> String {
    name.replace(['\t', '\n', '\r'], " ").trim().to_string()
}
