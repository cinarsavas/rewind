use std::env;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="dev.rewind.Rewind" version="1.0.0.0"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-env-changed=REWIND_REPO");
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    println!("cargo:rerun-if-changed=assets/rewind.ico");

    // Release builds on GitHub fetch add-ons from their own repository. A local build falls
    // back to the upstream project so the Add-ons page works without CI.
    let repo = env::var("REWIND_REPO").or_else(|_| env::var("GITHUB_REPOSITORY")).unwrap_or_else(|_| "cinarsavas/rewind".into());
    println!("cargo:rustc-env=REWIND_REPO={repo}");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Delay-load obs.dll so main() can fetch the OBS runtime on first launch before the
        // first OBS symbol is touched.
        libobs_bootstrapper::build::emit_windows_obs_delay_load();
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // A proper icon, version info and manifest make the exe look like what it is, which
        // also helps antivirus heuristics that distrust anonymous binaries.
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/rewind.ico")
            .set("ProductName", "Rewind")
            .set("FileDescription", "Rewind - instant replay recorder")
            .set("InternalName", "Rewind")
            .set("OriginalFilename", "Rewind.exe")
            .set("LegalCopyright", "GPL-3.0-or-later. Capture by libobs (OBS Studio).")
            .set_manifest(MANIFEST);
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed Windows resources: {e}");
        }
    }
}
