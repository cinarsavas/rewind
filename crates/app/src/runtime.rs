//! First-run provisioning of the OBS runtime (obs.dll + plugins), downloaded from the
//! official OBS Studio GitHub release and SHA-256 verified by `libobs-bootstrapper`.

use libobs_bootstrapper::{ObsBootstrapper, ObsBootstrapperOptions};

pub fn is_installed() -> bool {
    ObsBootstrapper::is_valid_installation().unwrap_or(false)
}

/// Blocking download + extract. Run on a worker thread; it takes a while on first launch.
pub fn install_blocking() -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("starting async runtime: {e}"))?;
    rt.block_on(ObsBootstrapper::bootstrap(&ObsBootstrapperOptions::default()))
        .map(|_| ())
        .map_err(|e| format!("downloading the OBS runtime failed: {e}"))
}
