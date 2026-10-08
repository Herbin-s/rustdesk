use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;

const FEATURE_IDS: [&str; 4] = [
    "window-targeting",
    "memory-watchdog",
    "headless-terminal",
    "headless-file-transfer",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FeatureState {
    enabled: bool,
    effective: Option<bool>,
    reason: String,
}

#[derive(Debug, Serialize)]
struct SettingsResponse {
    ok: bool,
    error: String,
    features: BTreeMap<String, FeatureState>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LiveFeatureStatus {
    window_targeting: bool,
    memory_watchdog: bool,
    memory_reason: String,
}

pub(crate) async fn get_settings() -> String {
    #[cfg(target_os = "macos")]
    let response = macos::settings(String::new()).await;
    #[cfg(not(target_os = "macos"))]
    let response = unsupported();
    serialize_response(response)
}

pub(crate) async fn set_feature(feature: String, enabled: bool) -> String {
    #[cfg(target_os = "macos")]
    let response = macos::set_feature(&feature, enabled).await;
    #[cfg(not(target_os = "macos"))]
    let response = {
        let _ = (feature, enabled);
        unsupported()
    };
    serialize_response(response)
}

fn serialize_response(response: SettingsResponse) -> String {
    // These fields contain only JSON strings, booleans and maps.
    serde_json::json!({
        "ok": response.ok,
        "error": response.error,
        "features": response.features,
    })
    .to_string()
}

#[cfg(not(target_os = "macos"))]
fn unsupported() -> SettingsResponse {
    let error = "RDH feature controls are supported on macOS only".to_owned();
    SettingsResponse {
        ok: false,
        error: error.clone(),
        features: FEATURE_IDS
            .iter()
            .map(|id| {
                (
                    (*id).to_owned(),
                    FeatureState {
                        enabled: true,
                        effective: Some(false),
                        reason: error.clone(),
                    },
                )
            })
            .collect(),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn headless_exit(feature: &str) -> Option<crate::rdh_cli::CliExit> {
    #[cfg(target_os = "macos")]
    {
        headless_preference_exit(
            feature,
            macos::read_preferences().map(|preferences| preferences.enabled(feature)),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = feature;
        None
    }
}

#[cfg(target_os = "macos")]
fn headless_preference_exit(
    feature: &str,
    enabled: Result<bool, String>,
) -> Option<crate::rdh_cli::CliExit> {
    let error = match enabled {
        Ok(true) => return None,
        Ok(false) => format!("RDH feature {feature} is disabled in settings"),
        Err(error) => error,
    };
    Some(crate::rdh_cli::CliExit {
        status: 2,
        stdout: String::new(),
        stderr: format!("{error}\n"),
    })
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{memory_watchdog_enabled, write_config};

#[cfg(target_os = "macos")]
pub(crate) fn live_status() -> LiveFeatureStatus {
    let (memory_watchdog, memory_reason) = crate::server::memory_watchdog::feature_status();
    LiveFeatureStatus {
        window_targeting: crate::window_targeting::status().mode
            == crate::window_targeting::WindowTargetingMode::Rules,
        memory_watchdog,
        memory_reason,
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::{self, Write},
        os::unix::fs::OpenOptionsExt,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(default, deny_unknown_fields)]
    pub(super) struct Preferences {
        memory_watchdog: bool,
        headless_terminal: bool,
        headless_file_transfer: bool,
    }

    impl Default for Preferences {
        fn default() -> Self {
            Self {
                memory_watchdog: true,
                headless_terminal: true,
                headless_file_transfer: true,
            }
        }
    }

    impl Preferences {
        pub(super) fn enabled(&self, feature: &str) -> bool {
            match feature {
                "memory-watchdog" => self.memory_watchdog,
                "headless-terminal" => self.headless_terminal,
                "headless-file-transfer" => self.headless_file_transfer,
                _ => false,
            }
        }

        fn set(&mut self, feature: &str, enabled: bool) -> Result<(), String> {
            match feature {
                "memory-watchdog" => self.memory_watchdog = enabled,
                "headless-terminal" => self.headless_terminal = enabled,
                "headless-file-transfer" => self.headless_file_transfer = enabled,
                _ => return Err(format!("Unknown RDH feature: {feature}")),
            }
            Ok(())
        }
    }

    fn preferences_path() -> Result<PathBuf, String> {
        crate::window_targeting::config_path()
            .map(|path| path.with_file_name("rdh-features.toml"))
            .map_err(|error| error.to_string())
    }

    fn read_preferences_at(path: &Path) -> Result<Preferences, String> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|_| "Invalid RDH feature preferences; features are disabled".to_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(error) => Err(format!("Read RDH feature preferences: {error}")),
        }
    }

    pub(super) fn read_preferences() -> Result<Preferences, String> {
        read_preferences_at(&preferences_path()?)
    }

    pub(crate) fn memory_watchdog_enabled() -> Result<bool, String> {
        Ok(read_preferences()?.memory_watchdog)
    }

    pub(crate) fn write_config(path: &Path, text: &str) -> Result<(), String> {
        static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
        let parent = path
            .parent()
            .ok_or_else(|| "RDH configuration directory is unavailable".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| format!("Create RDH directory: {error}"))?;
        let permissions = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => Some(metadata.permissions()),
            Ok(_) => return Err("RDH configuration is not a regular file".to_owned()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Inspect RDH configuration: {error}")),
        };
        let temporary = parent.join(format!(
            ".rdh-write-{}-{}",
            std::process::id(),
            NEXT_WRITE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| format!("Create RDH configuration replacement: {error}"))?;
        let result = (|| {
            if let Some(permissions) = permissions {
                file.set_permissions(permissions)?;
            }
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if let Err(error) = result {
            if let Err(cleanup_error) = fs::remove_file(&temporary) {
                hbb_common::log::warn!("RDH configuration cleanup failed: {cleanup_error}");
            }
            return Err(format!("Write RDH configuration: {error}"));
        }
        Ok(())
    }

    pub(super) async fn set_feature(feature: &str, enabled: bool) -> SettingsResponse {
        let update = if feature == "window-targeting" {
            crate::window_targeting::set_enabled_on_disk(enabled).map_err(|error| error.to_string())
        } else {
            (|| {
                let path = preferences_path()?;
                let mut preferences = read_preferences_at(&path)?;
                preferences.set(feature, enabled)?;
                let text = toml::to_string(&preferences)
                    .map_err(|error| format!("Encode RDH feature preferences: {error}"))?;
                write_config(&path, &text)
            })()
        };
        let error = match update {
            Err(error) => error,
            Ok(()) if feature == "window-targeting" => {
                match crate::ipc::reload_window_targeting_async().await {
                    Ok(response) if response.ok => String::new(),
                    Ok(response) => response.lines.join("; "),
                    Err(error) => format!("Preference saved; live window reload failed: {error}"),
                }
            }
            Ok(()) => String::new(),
        };
        settings(error).await
    }

    pub(super) async fn settings(mut error: String) -> SettingsResponse {
        let preferences = read_preferences();
        if let Err(read_error) = &preferences {
            append_error(&mut error, read_error);
        }
        let window_enabled = crate::window_targeting::enabled_on_disk();
        if let Err(read_error) = &window_enabled {
            append_error(&mut error, &read_error.to_string());
        }
        let live = crate::ipc::request_rdh_feature_status().await;
        let service_reason = if live.is_err() {
            "service-unavailable"
        } else {
            ""
        };
        let mut features = BTreeMap::new();
        for feature in FEATURE_IDS {
            let enabled = if feature == "window-targeting" {
                window_enabled.as_ref().copied().unwrap_or(false)
            } else {
                preferences
                    .as_ref()
                    .map(|preferences| preferences.enabled(feature))
                    .unwrap_or(false)
            };
            let (effective, reason) = match feature {
                "window-targeting" => (
                    live.as_ref().ok().map(|status| status.window_targeting),
                    service_reason.to_owned(),
                ),
                "memory-watchdog" => (
                    live.as_ref().ok().map(|status| status.memory_watchdog),
                    live.as_ref()
                        .map(|status| status.memory_reason.clone())
                        .unwrap_or_else(|_| service_reason.to_owned()),
                ),
                _ => (
                    Some(enabled),
                    if preferences.is_err() {
                        "invalid-preferences"
                    } else if !enabled {
                        "disabled"
                    } else {
                        ""
                    }
                    .to_owned(),
                ),
            };
            features.insert(
                feature.to_owned(),
                FeatureState {
                    enabled,
                    effective,
                    reason,
                },
            );
        }
        SettingsResponse {
            ok: error.is_empty(),
            error,
            features,
        }
    }

    fn append_error(error: &mut String, next: &str) {
        if !error.is_empty() {
            error.push_str("; ");
        }
        error.push_str(next);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn preferences_preserve_independence_and_fail_closed_on_invalid_input() {
            let mut preferences: Preferences = toml::from_str("").unwrap();
            assert!(preferences.memory_watchdog);
            assert!(preferences.headless_terminal);
            preferences.set("headless-terminal", false).unwrap();
            assert!(!preferences.headless_terminal);
            assert!(preferences.headless_file_transfer);
            assert!(preferences.memory_watchdog);
            let encoded = toml::to_string(&preferences).unwrap();
            let restored: Preferences = toml::from_str(&encoded).unwrap();
            assert!(!restored.headless_terminal);
            assert!(restored.headless_file_transfer);
            assert!(toml::from_str::<Preferences>("headless_terminal = 'false'").is_err());
            assert!(toml::from_str::<Preferences>("unknown_feature = false").is_err());
            assert!(preferences.set("product-identity", false).is_err());
        }

        #[test]
        fn disabled_or_unreadable_headless_features_return_stderr_and_usage_status() {
            for feature in ["headless-terminal", "headless-file-transfer"] {
                assert!(headless_preference_exit(feature, Ok(true)).is_none());
                for permission in [Ok(false), Err("Unreadable preferences".to_owned())] {
                    let exit = headless_preference_exit(feature, permission).unwrap();
                    assert_eq!(exit.status, 2);
                    assert!(exit.stdout.is_empty());
                    assert!(!exit.stderr.is_empty());
                }
            }
        }

        #[test]
        fn atomic_preferences_round_trip_and_invalid_file_remains_unchanged() {
            let path = PathBuf::from(std::env::var("TMPDIR").unwrap())
                .join(format!("rdh-features-test-{}.toml", std::process::id()));
            assert!(read_preferences_at(&path).unwrap().headless_terminal);
            write_config(&path, "headless_terminal = false\n").unwrap();
            assert!(!read_preferences_at(&path).unwrap().headless_terminal);
            write_config(&path, "headless_terminal = 'invalid'\n").unwrap();
            assert!(read_preferences_at(&path).is_err());
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                "headless_terminal = 'invalid'\n"
            );
            fs::remove_file(path).unwrap();
        }
    }
}
