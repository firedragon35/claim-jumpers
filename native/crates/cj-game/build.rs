//! Windows resources for claim-jumpers.exe: the icon Explorer shows, and
//! the version information on the file's Details tab. Compiled with the
//! Windows SDK's rc.exe (installed with the MSVC build tools) and linked
//! in as a .res file, so no extra crate is needed. Without rc.exe the game
//! still builds, just without the file icon (a warning says so).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let icon = manifest.join("../../assets/icon/claim-jumpers.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", icon.display());
    let windows_msvc = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found (Windows SDK): building without the file icon and version info");
        return;
    };
    let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo"));
    let (rc_file, res) = (out.join("claim-jumpers.rc"), out.join("claim-jumpers.res"));
    fs::write(&rc_file, resource_script(&icon)).expect("write the .rc file");
    let status = Command::new(&rc)
        .args(["/nologo", "/fo"])
        .arg(&res)
        .arg(&rc_file)
        .status()
        .expect("run rc.exe");
    assert!(status.success(), "rc.exe failed on {}", rc_file.display());
    // link.exe takes a compiled .res file as an ordinary input.
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// The resource script: icon 1 (the one Explorer uses) and VERSIONINFO
/// from the crate's version.
fn resource_script(icon: &Path) -> String {
    let v = env::var("CARGO_PKG_VERSION").expect("set by cargo");
    let mut n: Vec<u16> = v.split(['.', '-']).filter_map(|p| p.parse().ok()).collect();
    n.resize(4, 0);
    let commas = format!("{},{},{},{}", n[0], n[1], n[2], n[3]);
    let icon = icon.display().to_string().replace('\\', "\\\\");
    format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {commas}
PRODUCTVERSION {commas}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Claim Jumpers"
      VALUE "ProductName", "Claim Jumpers"
      VALUE "FileVersion", "{v}"
      VALUE "ProductVersion", "{v}"
      VALUE "OriginalFilename", "claim-jumpers.exe"
      VALUE "InternalName", "claim-jumpers"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

/// rc.exe: from $RC, then PATH, then the newest Windows 10/11 SDK.
fn find_rc() -> Option<PathBuf> {
    if let Ok(p) = env::var("RC") {
        return Some(PathBuf::from(p));
    }
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    };
    let root = PathBuf::from(
        env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into()),
    )
    .join(r"Windows Kits\10\bin");
    let mut kits: Vec<PathBuf> = fs::read_dir(&root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("10."))
        })
        .collect();
    // Versions like 10.0.26100.0 sort correctly as numbers, not as text.
    kits.sort_by_key(|p| {
        p.file_name().map(|n| {
            n.to_string_lossy()
                .split('.')
                .map(|x| x.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        })
    });
    kits.into_iter()
        .rev()
        .map(|k| k.join(arch).join("rc.exe"))
        .find(|p| p.exists())
}
