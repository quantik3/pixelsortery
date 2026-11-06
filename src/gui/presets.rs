//! Presets: named settings, saved in a text file next to the program

use std::path::PathBuf;

const PRESETS_FILE_NAME: &str = "presets.txt";

/// `presets.txt` in the folder of the program, so the program folder can be copied together with the presets
fn portable_presets_file() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(PRESETS_FILE_NAME))
}

/// Fallback if the program folder is not writable (e.g. in "Program Files"):
/// `%APPDATA%\pixlflow\presets.txt` on Windows, `~/.config/pixlflow/presets.txt` elsewhere
fn user_presets_file() -> Option<PathBuf> {
    Some(config_dir()?.join("pixlflow").join(PRESETS_FILE_NAME))
}

/// Where the presets were saved before the program was renamed
fn old_presets_file() -> Option<PathBuf> {
    Some(config_dir()?.join("pixelsortery").join(PRESETS_FILE_NAME))
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// Reads the saved presets: one per line, "name<TAB>settings".
/// If there are several preset files (next to the program, in the user folder, from before the rename),
/// the most recently saved one is used. They all move to the program folder with the next save.
pub(crate) fn load_presets() -> Vec<(String, String)> {
    let newest = [portable_presets_file(), user_presets_file(), old_presets_file()]
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok()?;
            Some((modified, file))
        })
        .max_by_key(|(modified, _)| *modified);
    let Some(text) = newest.and_then(|(_, file)| std::fs::read_to_string(file).ok()) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, settings)| (name.to_string(), settings.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// Writes all presets next to the program, or to the user folder if that's not possible
pub(crate) fn save_presets(presets: &[(String, String)]) {
    let text: String = presets
        .iter()
        .map(|(name, settings)| format!("{}\t{}\n", clean_name(name), settings))
        .collect();

    let mut last_error = None;
    for file in [portable_presets_file(), user_presets_file()].into_iter().flatten() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&file, &text) {
            Ok(()) => {
                if last_error.is_some() {
                    eprintln!("Presets saved to '{}' instead", file.to_string_lossy());
                }
                return;
            }
            Err(e) => {
                eprintln!("Could not save presets to '{}': {}", file.to_string_lossy(), e);
                last_error = Some(e);
            }
        }
    }
}

/// Preset names may not contain tabs or line breaks (they separate the entries in the file)
pub(crate) fn clean_name(name: &str) -> String {
    name.replace(['\t', '\n', '\r'], " ").trim().to_string()
}
