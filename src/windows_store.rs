//! Read-only discovery of user-installed AppX/MSIX packages via Windows'
//! package registration and Start menu AUMID metadata.
//! Unlike Win32 software, these apps cannot be cloned by copying an .exe.
use crate::universal::{DiscoveredApp, Support};
use serde::Deserialize;
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
};

// CREATE_NO_WINDOW prevents PowerShell from opening a conhost/Windows Terminal
// window while Store/MSIX inventory runs in the background.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// AUMIDs must be one package-family identifier plus one application ID.
/// Keep it safe to append to shell:AppsFolder without passing untrusted syntax.
pub fn valid_aumid(input: &str) -> bool {
    let Some((family, app)) = input.split_once('!') else {
        return false;
    };
    !family.is_empty()
        && !app.is_empty()
        && input.len() <= 240
        && !app.contains('!')
        && input
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'.' | b'_' | b'-' | b'!'))
}

#[derive(Debug, Deserialize)]
struct StoreEntry {
    name: String,
    aumid: String,
    #[serde(default)]
    version: Option<String>,
}

/// The script is constant and never interpolates discovered data into commands.
/// It discovers only launchers for packages registered to the current user:
/// Get-AppxPackage -> manifest Application IDs -> Get-StartApps friendly names.
/// No admin rights or filesystem writes, no package installation/uninstallation.
const STORE_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$start = @{}
Get-StartApps | ForEach-Object {
  $key = [string]$_.AppID
  if ($key.Contains('!') -and -not $start.ContainsKey($key)) {
    $start[$key] = [string]$_.Name
  }
}
$results = New-Object 'System.Collections.Generic.List[Object]'
Get-AppxPackage | Where-Object {
  -not $_.IsFramework -and -not $_.IsResourcePackage
} | Select-Object -First 600 | ForEach-Object {
  $pkg = $_
  $family = [string]$pkg.PackageFamilyName
  if ([string]::IsNullOrWhiteSpace($family)) { return }
  $prefix = $family + '!'
  $hasLauncher = $false
  foreach ($key in $start.Keys) {
    if ($key.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      $hasLauncher = $true
      break
    }
  }
  if (-not $hasLauncher) { return }
  $manifest = Get-AppxPackageManifest -Package $pkg.PackageFullName
  if ($null -eq $manifest) { return }
  foreach ($application in @($manifest.Package.Applications.Application)) {
    if ($null -eq $application) { continue }
    $id = [string]$application.Id
    if ([string]::IsNullOrWhiteSpace($id)) { continue }
    $aumid = $family + '!' + $id
    if (-not $start.ContainsKey($aumid)) { continue }
    $display = $start[$aumid]
    if ([string]::IsNullOrWhiteSpace($display)) { continue }
    $results.Add([pscustomobject]@{
      name = $display
      aumid = $aumid
      version = [string]$pkg.Version
    })
  }
}
ConvertTo-Json -InputObject $results.ToArray() -Compress -Depth 3
"#;

fn parse_entries(json: &str) -> Vec<DiscoveredApp> {
    let Ok(entries) = serde_json::from_str::<Vec<StoreEntry>>(json) else {
        return Vec::new();
    };
    let mut apps = Vec::new();
    for entry in entries.into_iter().take(500) {
        let name = entry.name.trim();
        if name.is_empty() || name.len() > 160 || !valid_aumid(&entry.aumid) {
            continue;
        }
        apps.push(DiscoveredApp {
            name: name.to_string(),
            path: PathBuf::new(),
            support: Support::ReviewRequired,
            note: "Registered Windows AppX/MSIX application. Open normally through Windows; copying the package or creating separate sandbox profiles is not supported.".into(),
            origin: "Windows packaged app (AppX/MSIX)",
            // Launchable via its AUMID, not a runnable .exe.
            launchable: true,
            publisher: None,
            version: entry.version.filter(|v| !v.trim().is_empty()),
            store_id: Some(entry.aumid),
        });
    }
    apps
}

// Console-subsystem PowerShell children must never display Windows Terminal.
fn hidden_powershell(script: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command.stdin(Stdio::null());
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    command
}

pub fn discover_apps() -> Vec<DiscoveredApp> {
    let result = hidden_powershell(STORE_SCRIPT).output();
    match result {
        Ok(output) if output.status.success() && output.stdout.len() <= 2_000_000 => {
            parse_entries(&String::from_utf8_lossy(&output.stdout))
        }
        _ => Vec::new(),
    }
}

/// Launch a registered packaged app through Windows activation; no .exe copy.
pub fn open_app(aumid: &str) -> Result<(), String> {
    if !valid_aumid(aumid) {
        return Err("Invalid packaged application identifier.".into());
    }
    Command::new("explorer.exe")
        .arg(format!(r"shell:AppsFolder\{aumid}"))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Windows could not open this packaged application: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_powershell_captures_inventory_output() {
        let output = hidden_powershell("Write-Output 'softclone-store-discovery-probe'")
            .output()
            .expect("Windows PowerShell should be available");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "softclone-store-discovery-probe"
        );
    }

    #[test]
    fn only_valid_app_user_model_ids_accepted() {
        assert!(valid_aumid("Example.App_abc!App"));
        assert!(!valid_aumid(""));
        assert!(!valid_aumid("AppWithoutFamily"));
        assert!(!valid_aumid("Family!App!Extra"));
        assert!(!valid_aumid("Family!App;calc.exe"));
        assert!(!valid_aumid("Family!App\\Other"));
        assert!(!valid_aumid("Family!App\nSomething"));
    }

    #[test]
    fn packaged_apps_are_openable_not_direct_clone_sources() {
        let json = r#"[{"name":"CapCut","aumid":"Bytedance.CapCut_abc!App","version":"8.7.0"},{"name":"Bad","aumid":"App!Open;bad"}]"#;
        let result = parse_entries(json);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "CapCut");
        assert!(result[0].launchable);
        assert_eq!(result[0].support, Support::ReviewRequired);
        assert!(result[0].path.as_os_str().is_empty());
        assert!(result[0].store_id.is_some());
    }
}
