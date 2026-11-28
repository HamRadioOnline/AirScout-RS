//! Shared build-script support for embedding an icon in a Windows executable.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn embed_icon(ico: &Path, bin: &str) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../packaging/windows/embed-icon.rs");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-env-changed=PATH");

    let target = env::var("TARGET").expect("Cargo sets TARGET");
    let (tool, tool_name, resource, args): (_, _, _, &[&str]) =
        match env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => (
                find_rc(),
                "rc.exe".to_owned(),
                "app-icon.res",
                &["/nologo", "/fo", "app-icon.res", "app-icon.rc"],
            ),
            Ok("gnu") => {
                let name = format!(
                    "{}-windres",
                    target.replace("-pc-windows-gnu", "-w64-mingw32")
                );
                (
                    find_on_path(&name),
                    name,
                    "app-icon.o",
                    &["-J", "rc", "-O", "coff", "app-icon.rc", "app-icon.o"],
                )
            }
            _ => {
                println!("cargo:warning=No Windows icon compiler configured for {target}");
                return;
            }
        };
    let Some(tool) = tool else {
        println!("cargo:warning=Could not find {tool_name} for {target}; skipping {bin} icon");
        return;
    };

    // Work in OUT_DIR with fixed file names so the resource script needs no path escaping.
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    fs::copy(ico, out.join("app-icon.ico")).expect("copy application icon to OUT_DIR");
    fs::write(out.join("app-icon.rc"), "1 ICON \"app-icon.ico\"\n")
        .expect("write application resource script");
    let status = Command::new(&tool)
        .args(args)
        .current_dir(&out)
        .status()
        .unwrap_or_else(|err| panic!("could not run {}: {err}", tool.display()));
    assert!(status.success(), "{} failed: {status}", tool.display());
    println!(
        "cargo:rustc-link-arg-bin={bin}={}",
        out.join(resource).display()
    );
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let name = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
        .and_then(|path| std::path::absolute(path).ok())
}

fn find_rc() -> Option<PathBuf> {
    for var in ["WindowsSdkVerBinPath", "WindowsSdkDir", "ProgramFiles(x86)"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    if let Some(rc) = find_on_path("rc.exe") {
        return Some(rc);
    }

    // The compiler is an SDK host tool, even when compiling for a different architecture.
    let host = env::var("HOST").ok()?;
    let arch = match host.split('-').next()? {
        "aarch64" => "arm64",
        "i686" => "x86",
        "x86_64" => "x64",
        _ => return None,
    };
    if let Some(rc) = env::var_os("WindowsSdkVerBinPath")
        .map(|path| PathBuf::from(path).join(arch).join("rc.exe"))
        .filter(|path| path.is_file())
    {
        return Some(rc);
    }

    // Cargo can find MSVC's linker even when the developer shell has not set PATH.
    let sdk = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .or_else(|| {
            Some(PathBuf::from(env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10"))
        })?;
    let bin = sdk.join("bin");
    fs::read_dir(&bin)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let version = entry
                .file_name()
                .to_str()?
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            let rc = entry.path().join(arch).join("rc.exe");
            rc.is_file().then_some((version, rc))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, rc)| rc)
}
