use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::{
    GAME_EXECUTABLE, InstallationSnapshot, InstallerState, LanguageSpec, MOD_RELATIVE_PATH,
    Preferences, resolve_language,
};
use crate::runtime::RuntimeContext;
use pelite::pe64::{Pe, PeFile};
use pelite::resources::Name;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub trait AppPaths {
    fn data_dir(&self) -> AppResult<PathBuf>;
    fn cache_dir(&self) -> AppResult<PathBuf>;
}

impl AppPaths for RuntimeContext {
    fn data_dir(&self) -> AppResult<PathBuf> {
        Ok(self.data_dir.clone())
    }

    fn cache_dir(&self) -> AppResult<PathBuf> {
        Ok(self.cache_dir.clone())
    }
}

pub fn app_data_dir<C: AppPaths>(context: &C) -> AppResult<PathBuf> {
    context.data_dir()
}

pub fn app_cache_dir<C: AppPaths>(context: &C) -> AppResult<PathBuf> {
    context.cache_dir()
}

pub fn default_install_directory() -> AppResult<String> {
    let executable = std::env::current_exe()
        .map_err(|error| AppError::io("Could not locate the launcher executable", error))?;
    let directory = executable
        .parent()
        .ok_or_else(|| AppError::message("The launcher executable has no parent folder."))?;
    Ok(directory.to_string_lossy().into_owned())
}

pub fn is_existing_installation_directory(path: &Path) -> bool {
    path.join(GAME_EXECUTABLE).is_file() || state_path(path).is_file()
}

pub async fn load_preferences<C: AppPaths>(context: &C) -> AppResult<Preferences> {
    let path = app_data_dir(context)?.join("preferences.json");
    if !path.exists() {
        return Ok(Preferences::default());
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| AppError::io("Could not read preferences", error))?;
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

pub async fn save_preferences<C: AppPaths>(
    context: &C,
    preferences: &Preferences,
) -> AppResult<()> {
    let root = app_data_dir(context)?;
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| AppError::io("Could not create the application data folder", error))?;
    let bytes = serde_json::to_vec_pretty(preferences)?;
    write_atomic(&root.join("preferences.json"), &bytes).await
}

pub async fn load_state(install_root: &Path) -> AppResult<Option<InstallerState>> {
    let path = state_path(install_root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| AppError::io("Could not read the installation state", error))?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub async fn save_state(install_root: &Path, state: &InstallerState) -> AppResult<()> {
    let path = state_path(install_root);
    let bytes = serde_json::to_vec_pretty(state)?;
    write_atomic(&path, &bytes).await
}

pub async fn inspect_installation(path: &str) -> AppResult<InstallationSnapshot> {
    if path.trim().is_empty() {
        return Ok(InstallationSnapshot::missing(
            "Choose an installation folder to get started.",
        ));
    }
    let root = PathBuf::from(path.trim());
    let game_found = root.join(GAME_EXECUTABLE).is_file();
    let state = load_state(&root).await?;
    let steam_language = installed_language(&root, state.as_ref())
        .await
        .map(|language| language.steam_language.to_owned());
    let Some(state) = state else {
        return Ok(InstallationSnapshot {
            status: if game_found {
                "unmanaged"
            } else {
                "notInstalled"
            }
            .into(),
            message: if game_found {
                "Destiny 2 was found, but this launcher has not installed Sunrise here."
            } else {
                "No managed Sunrise installation was found in this folder."
            }
            .into(),
            game_found,
            installed_release: None,
            installed_release_digest: None,
            installed_at: None,
            local_file_changed: false,
            steam_language,
            missions_commit: None,
        });
    };

    let dll = mod_path(&root);
    if !dll.is_file() {
        return Ok(InstallationSnapshot {
            status: "needsRepair".into(),
            message: "The Sunrise DLL is missing. Run Repair.".into(),
            game_found,
            installed_release: Some(state.release_tag),
            installed_release_digest: state.release_asset_digest,
            installed_at: Some(state.installed_at_utc),
            local_file_changed: true,
            steam_language,
            missions_commit: state.missions_commit,
        });
    }

    let expected = state.installed_dll_sha256.clone();
    let actual = hash_file(&dll).await?;
    let changed = !expected.eq_ignore_ascii_case(&actual);
    Ok(InstallationSnapshot {
        status: if changed { "modified" } else { "installed" }.into(),
        message: if changed {
            "The installed DLL differs from the recorded release. Run Repair or Update."
        } else {
            "Sunrise is installed and ready."
        }
        .into(),
        game_found,
        installed_release: Some(state.release_tag),
        installed_release_digest: state.release_asset_digest,
        installed_at: Some(state.installed_at_utc),
        local_file_changed: changed,
        steam_language,
        missions_commit: state.missions_commit,
    })
}

pub async fn hash_file(path: &Path) -> AppResult<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path)
            .map_err(|error| AppError::io("Could not open a file for verification", error))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| AppError::io("Could not verify a file", error))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|error| {
        AppError::message(format!("File verification stopped unexpectedly: {error}"))
    })?
}

pub fn state_path(install_root: &Path) -> PathBuf {
    install_root.join(".sunrise").join("install-state.json")
}

pub fn mod_path(install_root: &Path) -> PathBuf {
    MOD_RELATIVE_PATH
        .iter()
        .fold(install_root.to_path_buf(), |path, part| path.join(part))
}

pub fn settings_path(install_root: &Path) -> PathBuf {
    install_root
        .join("bin")
        .join("x64")
        .join("Sunrise")
        .join("settings.json")
}

/** The recorded install language, then the settings.json language, else none. */
pub async fn installed_language(
    install_root: &Path,
    state: Option<&InstallerState>,
) -> Option<&'static LanguageSpec> {
    if let Some(language) = state.and_then(|state| state.steam_language.as_deref()) {
        return Some(resolve_language(language));
    }
    let bytes = tokio::fs::read(settings_path(install_root)).await.ok()?;
    let settings = serde_json::from_slice::<Value>(without_byte_order_mark(&bytes)).ok()?;
    settings["steam"]["language"].as_str().map(resolve_language)
}

/**
 * Builds settings.json from the defaults inside `dll`, with the Steam language set.
 * The DLL replaces a file older than its settings version, which drops the language.
 */
pub async fn prepare_sunrise_settings(
    install_root: &Path,
    dll: &Path,
    steam_language: &str,
    reset: bool,
) -> AppResult<Vec<u8>> {
    let dll_bytes = tokio::fs::read(dll)
        .await
        .map_err(|error| AppError::io("Could not read the Sunrise DLL", error))?;
    let defaults = bundled_settings(&dll_bytes)?;
    let path = settings_path(install_root);
    let existing = if !reset && path.is_file() {
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| AppError::io("Could not read Sunrise settings.json", error))?;
        let Ok(Value::Object(existing)) =
            serde_json::from_slice::<Value>(without_byte_order_mark(&bytes))
        else {
            return Err(AppError::message(
                "Sunrise settings.json could not be read.",
            ));
        };
        Some(existing)
    } else {
        None
    };
    let settings = merge_settings(defaults, existing, steam_language)?;
    Ok(serde_json::to_vec_pretty(&settings)?)
}

pub async fn write_sunrise_settings(install_root: &Path, settings: &[u8]) -> AppResult<()> {
    write_atomic(&settings_path(install_root), settings).await
}

fn merge_settings(
    defaults: Map<String, Value>,
    existing: Option<Map<String, Value>>,
    steam_language: &str,
) -> AppResult<Map<String, Value>> {
    let mut settings = match existing {
        Some(mut existing) if settings_version(&existing) >= settings_version(&defaults) => {
            add_missing_defaults(&mut existing, &defaults);
            existing
        }
        _ => defaults,
    };
    let Some(Value::Object(steam)) = settings.get_mut("steam") else {
        return Err(AppError::message(
            "Sunrise settings.json has no valid Steam section.",
        ));
    };
    steam.insert("language".into(), Value::String(steam_language.into()));
    Ok(settings)
}

// The Sunrise DLL embeds its default settings as RCDATA resource 101.
const DEFAULT_SETTINGS_RESOURCE: u32 = 101;
const RT_RCDATA: u32 = 10;

fn bundled_settings(dll: &[u8]) -> AppResult<Map<String, Value>> {
    let invalid = || AppError::message("The Sunrise DLL does not contain valid default settings.");
    let pe = PeFile::from_bytes(dll).map_err(|_| invalid())?;
    let document = pe
        .resources()
        .ok()
        .and_then(|resources| {
            resources
                .find_resource(&[Name::Id(RT_RCDATA), Name::Id(DEFAULT_SETTINGS_RESOURCE)])
                .ok()
        })
        .ok_or_else(invalid)?;
    let Ok(Value::Object(settings)) =
        serde_json::from_slice::<Value>(without_byte_order_mark(document))
    else {
        return Err(invalid());
    };
    if settings_version(&settings) == 0 || !settings.get("steam").is_some_and(Value::is_object) {
        return Err(invalid());
    }
    Ok(settings)
}

// A missing or unreadable version is zero, as in the DLL.
fn settings_version(settings: &Map<String, Value>) -> u32 {
    settings
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .unwrap_or(0)
}

/** Missing keys take the default; existing values stay, including null and arrays. */
fn add_missing_defaults(settings: &mut Map<String, Value>, defaults: &Map<String, Value>) {
    for (key, default) in defaults {
        match (settings.get_mut(key), default) {
            (None, _) => {
                settings.insert(key.clone(), default.clone());
            }
            (Some(Value::Object(existing)), Value::Object(default)) => {
                add_missing_defaults(existing, default);
            }
            _ => {}
        }
    }
}

fn without_byte_order_mark(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::message("The data file has no parent folder."))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| AppError::io("Could not create a data folder", error))?;
    let temporary = parent.join(format!(
        ".{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| AppError::io("Could not write local installer data", error))?;
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| AppError::io("Could not replace local installer data", error))?;
    }
    tokio::fs::rename(&temporary, path)
        .await
        .map_err(|error| AppError::io("Could not finish writing local installer data", error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::{installed_language, merge_settings, settings_path};
    use crate::models::InstallerState;

    fn object(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(object) => object,
            _ => panic!("test value is not an object"),
        }
    }

    fn defaults() -> Map<String, Value> {
        object(json!({
            "version": 18,
            "video": { "fieldOfView": 70, "vsync": true },
            "steam": { "language": "english", "user": { "name": "Guardian" } }
        }))
    }

    #[test]
    fn current_settings_keep_their_values_and_gain_new_defaults() {
        let existing = object(json!({
            "version": 18,
            "video": { "fieldOfView": 90 },
            "steam": { "language": "english", "offline": true }
        }));
        let merged = merge_settings(defaults(), Some(existing), "french").expect("merged");
        assert_eq!(merged["version"], 18);
        assert_eq!(merged["video"]["fieldOfView"], 90);
        assert_eq!(merged["video"]["vsync"], true);
        assert_eq!(merged["steam"]["offline"], true);
        assert_eq!(merged["steam"]["user"]["name"], "Guardian");
        assert_eq!(merged["steam"]["language"], "french");
    }

    #[test]
    fn old_or_missing_settings_are_rebuilt_with_the_language() {
        for existing in [
            None,
            Some(object(json!({ "steam": { "language": "german" } }))),
            Some(object(
                json!({ "version": 17, "video": { "fieldOfView": 90 } }),
            )),
        ] {
            let merged = merge_settings(defaults(), existing, "german").expect("merged");
            assert_eq!(merged["version"], 18);
            assert_eq!(merged["video"]["fieldOfView"], 70);
            assert_eq!(merged["steam"]["language"], "german");
        }
    }

    #[tokio::test]
    async fn installed_language_prefers_state_then_settings() {
        let temporary = tempfile::tempdir().expect("temporary installation");
        assert!(installed_language(temporary.path(), None).await.is_none());

        let settings = settings_path(temporary.path());
        std::fs::create_dir_all(settings.parent().expect("settings folder"))
            .expect("settings folder");
        std::fs::write(
            &settings,
            b"\xEF\xBB\xBF{\"steam\":{\"language\":\"polish\"}}",
        )
        .expect("settings file");
        let legacy: InstallerState = serde_json::from_value(json!({
            "releaseTag": "v1",
            "releaseAsset": "steam_api64.dll",
            "releaseAssetDigest": null,
            "installedDllSha256": "aa",
            "installedAtUtc": "2026-08-26T12:00:00+00:00",
            "manifests": {}
        }))
        .expect("legacy state");
        assert_eq!(
            installed_language(temporary.path(), Some(&legacy))
                .await
                .map(|language| language.steam_language),
            Some("polish")
        );

        let recorded = InstallerState {
            steam_language: Some("german".into()),
            ..legacy
        };
        assert_eq!(
            installed_language(temporary.path(), Some(&recorded))
                .await
                .map(|language| language.steam_language),
            Some("german")
        );
    }
}
