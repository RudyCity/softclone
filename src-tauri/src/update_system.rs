//! Local updater for official GitHub stable releases.
//! No arbitrary URLs, silent installation or unsigned in-place replacement.
//! The user explicitly opens an installer after its SHA-256 is checked against
//! the official release's SHA256SUMS.txt, with a second hash check on launch.
use crate::{blocking, guard, DesktopResult};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::State;

const REPO: &str = "RudyCity/softclone";
const RELEASES_API: &str = "https://api.github.com/repos/RudyCity/softclone/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/RudyCity/softclone/releases";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_CHECKSUM_BYTES: u64 = 128 * 1024;
const MAX_ASSET_BYTES: u64 = 350 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    current_version: String,
    latest_version: String,
    update_available: bool,
    release_url: String,
    release_notes: String,
    asset_name: Option<String>,
    asset_size: Option<u64>,
    platform_supported: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    downloaded_bytes: u64,
    total_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedDownload {
    version: String,
    name: String,
    path: String,
    sha256: String,
    size: u64,
}

#[derive(Default)]
struct DownloadState {
    active: bool,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<UpdateProgress>,
    verified: Option<VerifiedDownload>,
}

#[derive(Clone, Default)]
pub struct UpdateManager {
    state: Arc<Mutex<DownloadState>>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

fn installed_version() -> DesktopResult<Version> {
    Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())
}

fn parse_release_version(tag: &str) -> DesktopResult<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| "GitHub provided an invalid stable release version.".into())
}

fn github_url(version: &Version, file: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{version}/{file}")
}

fn client() -> DesktopResult<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("SoftClone-Updater/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Cannot initialize secure connection: {error}"))
}

fn download_client() -> DesktopResult<Client> {
    Client::builder()
        .timeout(Duration::from_secs(900))
        .user_agent(format!("SoftClone-Updater/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Cannot initialize secure download: {error}"))
}

fn latest_release() -> DesktopResult<(Release, Version)> {
    let response = client()?
        .get(RELEASES_API)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("Could not check GitHub Releases: {error}"))?;
    let release: Release = serde_json::from_reader(response.take(MAX_METADATA_BYTES))
        .map_err(|error| format!("Invalid release information: {error}"))?;
    let version = parse_release_version(&release.tag_name)?;
    if release.draft || release.prerelease || !release.tag_name.starts_with('v') {
        return Err("Only published stable SoftClone releases are eligible.".into());
    }
    Ok((release, version))
}

fn package_name(version: &Version) -> Option<String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some(format!("SoftClone_{version}_x64-setup.exe")),
        ("macos", "aarch64") => Some(format!("SoftClone_{version}_aarch64.dmg")),
        ("macos", "x86_64") => Some(format!("SoftClone_{version}_x64.dmg")),
        ("linux", "x86_64") => Some(format!("SoftClone_{version}_amd64.AppImage")),
        _ => None,
    }
}

fn valid_asset(asset: &ReleaseAsset, version: &Version) -> bool {
    let Some(expected_name) = package_name(version) else {
        return false;
    };
    asset.name == expected_name
        && asset.browser_download_url == github_url(version, &expected_name)
        && asset.size > 0
        && asset.size <= MAX_ASSET_BYTES
}

fn release_asset<'a>(release: &'a Release, version: &Version) -> Option<&'a ReleaseAsset> {
    release
        .assets
        .iter()
        .find(|asset| valid_asset(asset, version))
}

fn checked_release() -> DesktopResult<(Release, Version)> {
    let (release, version) = latest_release()?;
    if version <= installed_version()? {
        return Err("SoftClone is already up to date.".into());
    }
    Ok((release, version))
}

fn checksum_from_manifest(text: &str, name: &str) -> DesktopResult<String> {
    let hashes: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            let (hash, filename) = line.split_once("  ")?;
            if filename.trim_start_matches('*') == name {
                Some(hash)
            } else {
                None
            }
        })
        .collect();
    if hashes.len() != 1
        || hashes[0].len() != 64
        || !hashes[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Official release checksum is missing or invalid.".into());
    }
    Ok(hashes[0].to_ascii_lowercase())
}

fn expected_sha256(version: &Version, name: &str) -> DesktopResult<String> {
    let url = github_url(version, "SHA256SUMS.txt");
    let response = client()?
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("Cannot retrieve official SHA-256 manifest: {error}"))?;
    let mut text = String::new();
    response
        .take(MAX_CHECKSUM_BYTES)
        .read_to_string(&mut text)
        .map_err(|error| format!("Cannot read SHA-256 manifest: {error}"))?;
    checksum_from_manifest(&text, name)
}

fn digest_file(path: &Path) -> DesktopResult<String> {
    let mut file = File::open(path).map_err(|error| format!("Cannot read installer: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[tauri::command]
pub async fn check_updates() -> DesktopResult<UpdateCheck> {
    blocking(move || {
        let current = installed_version()?;
        let (release, version) = latest_release()?;
        let asset = release_asset(&release, &version);
        let available = version > current;
        Ok(UpdateCheck {
            current_version: current.to_string(),
            latest_version: version.to_string(),
            update_available: available,
            release_url: format!("{RELEASES_PAGE}/tag/v{version}"),
            release_notes: release
                .body
                .as_deref()
                .unwrap_or_default()
                .chars()
                .take(5500)
                .collect(),
            asset_name: asset.map(|asset| asset.name.clone()),
            asset_size: asset.map(|asset| asset.size),
            platform_supported: asset.is_some(),
        })
    })
    .await
}

#[tauri::command]
pub fn update_download_progress(
    state: State<'_, UpdateManager>,
) -> DesktopResult<Option<UpdateProgress>> {
    Ok(guard(&state.state)?.progress.clone())
}

#[tauri::command]
pub fn cancel_update_download(state: State<'_, UpdateManager>) -> DesktopResult<()> {
    let inner = guard(&state.state)?;
    let token = inner
        .cancel
        .as_ref()
        .ok_or_else(|| "There is no update download in progress.".to_string())?;
    token.store(true, Ordering::Relaxed);
    Ok(())
}

fn perform_download(
    manager: &UpdateManager,
    token: &AtomicBool,
) -> DesktopResult<VerifiedDownload> {
    let (release, version) = checked_release()?;
    let asset = release_asset(&release, &version)
        .ok_or_else(|| "No compatible official installer exists for this platform.".to_string())?;
    let expected = expected_sha256(&version, &asset.name)?;
    if token.load(Ordering::Relaxed) {
        return Err("Update download cancelled.".into());
    }
    let storage = softclone::registry::app_data_dir().join("updates");
    fs::create_dir_all(&storage).map_err(|error| error.to_string())?;
    let destination = storage.join(&asset.name);
    let partial = storage.join(format!(".{}.{}.partial", asset.name, uuid::Uuid::new_v4()));
    let outcome = (|| -> DesktopResult<VerifiedDownload> {
        let response = download_client()?
            .get(&asset.browser_download_url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| format!("Cannot download update from GitHub: {error}"))?;
        let mut response = response.take(MAX_ASSET_BYTES + 1);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|error| format!("Cannot stage installer: {error}"))?;
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 128 * 1024];
        loop {
            if token.load(Ordering::Relaxed) {
                return Err("Update download cancelled.".into());
            }
            let count = response
                .read(&mut buffer)
                .map_err(|error| format!("Update download was interrupted: {error}"))?;
            if count == 0 {
                break;
            }
            total = total.saturating_add(count as u64);
            if total > MAX_ASSET_BYTES || total > asset.size {
                return Err("Downloaded installer exceeds its declared size.".into());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..count]);
            if let Ok(mut inner) = manager.state.lock() {
                inner.progress = Some(UpdateProgress {
                    downloaded_bytes: total,
                    total_bytes: asset.size,
                });
            }
        }
        output.sync_all().map_err(|error| error.to_string())?;
        drop(output);
        if total != asset.size {
            return Err("Downloaded installer was incomplete.".into());
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != expected {
            return Err("Installer SHA-256 verification failed. Update was discarded.".into());
        }
        // Refuse to replace a previously downloaded file (or a symlink).
        if destination.exists() || destination.symlink_metadata().is_ok() {
            if destination.is_symlink() {
                return Err("Existing update destination is a symbolic link.".into());
            }
            let old = digest_file(&destination)?;
            if old != expected {
                fs::remove_file(&destination).map_err(|error| error.to_string())?;
                fs::rename(&partial, &destination).map_err(|error| error.to_string())?;
            }
        } else {
            fs::rename(&partial, &destination).map_err(|error| error.to_string())?;
        }
        Ok(VerifiedDownload {
            version: version.to_string(),
            name: asset.name.clone(),
            path: destination.to_string_lossy().into_owned(),
            sha256: expected,
            size: asset.size,
        })
    })();
    let _ = fs::remove_file(&partial);
    outcome
}

#[tauri::command]
pub async fn download_update(state: State<'_, UpdateManager>) -> DesktopResult<VerifiedDownload> {
    let manager = state.inner().clone();
    let token = Arc::new(AtomicBool::new(false));
    {
        let mut inner = guard(&manager.state)?;
        if inner.active {
            return Err("An update download is already running.".into());
        }
        inner.active = true;
        inner.cancel = Some(token.clone());
        inner.progress = None;
        inner.verified = None;
    }
    blocking(move || {
        let result = perform_download(&manager, &token);
        if let Ok(mut inner) = manager.state.lock() {
            inner.active = false;
            inner.cancel = None;
            inner.progress = None;
            inner.verified = result.as_ref().ok().cloned();
        }
        result
    })
    .await
}

#[tauri::command]
pub async fn open_downloaded_update(state: State<'_, UpdateManager>) -> DesktopResult<()> {
    let manager = state.inner().clone();
    blocking(move || {
        let verified = guard(&manager.state)?.verified.clone().ok_or_else(|| {
            "Download and verify an update before opening its installer.".to_string()
        })?;
        let expected_version =
            Version::parse(&verified.version).map_err(|error| error.to_string())?;
        if expected_version <= installed_version()? {
            return Err("This update is not newer than the installed version.".into());
        }
        let expected_name = package_name(&expected_version)
            .ok_or_else(|| "Updates are unsupported on this architecture.".to_string())?;
        if verified.name != expected_name {
            return Err("Verified package no longer matches this device.".into());
        }
        let path = PathBuf::from(&verified.path);
        if path
            != softclone::registry::app_data_dir()
                .join("updates")
                .join(&expected_name)
        {
            return Err("Installer is outside SoftClone's managed updates directory.".into());
        }
        if !path.is_file() || path.is_symlink() {
            return Err("Verified installer is missing or is a symbolic link.".into());
        }
        if digest_file(&path)? != verified.sha256 {
            return Err("Installer changed after verification. Download it again.".into());
        }
        #[cfg(target_os = "windows")]
        let result = Command::new(&path).spawn();
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(&path).spawn();
        #[cfg(target_os = "linux")]
        let result = Command::new("xdg-open").arg(&path).spawn();
        result
            .map(|_| ())
            .map_err(|error| format!("Could not open verified installer: {error}"))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_manifest_requires_exact_unique_filename() {
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let manifest = format!("{first}  SoftClone_1.2.3_x64-setup.exe\n{second}  other.exe\n");
        assert_eq!(
            checksum_from_manifest(&manifest, "SoftClone_1.2.3_x64-setup.exe").unwrap(),
            first
        );
        assert!(checksum_from_manifest(&manifest, "SoftClone_1.2.3_x64-setup.exe.old").is_err());
        assert!(checksum_from_manifest("garbage", "file.exe").is_err());
        let duplicate = format!("{manifest}{first}  SoftClone_1.2.3_x64-setup.exe\n");
        assert!(checksum_from_manifest(&duplicate, "SoftClone_1.2.3_x64-setup.exe").is_err());
    }

    #[test]
    fn only_expected_platform_assets_accepted() {
        let version = Version::parse("1.2.3").unwrap();
        if let Some(name) = package_name(&version) {
            let official = ReleaseAsset {
                name: name.clone(),
                browser_download_url: github_url(&version, &name),
                size: 1024,
            };
            assert!(valid_asset(&official, &version));
            let tampered = ReleaseAsset {
                browser_download_url: "https://example.org/softclone.exe".into(),
                ..official
            };
            assert!(!valid_asset(&tampered, &version));
        }
    }

    #[test]
    fn only_stable_semver_tags_accepted() {
        assert_eq!(
            parse_release_version("v1.2.3").unwrap().to_string(),
            "1.2.3"
        );
        assert!(parse_release_version("latest").is_err());
    }
}
