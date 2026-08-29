//! Exports the browser asset set pinned by `truapi-pvm-host`.

use std::path::PathBuf;

use truapi_pvm_host::browser_assets;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let flag = arguments.next();
    let output = arguments.next();
    if flag.as_deref() != Some(std::ffi::OsStr::new("--output"))
        || output.is_none()
        || arguments.next().is_some()
    {
        return Err("usage: pvm-assets-export --output <directory>".into());
    }

    let output = PathBuf::from(output.expect("validated output argument"));
    std::fs::create_dir_all(&output)?;
    for asset in browser_assets() {
        let destination = output.join(asset.path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(destination, asset.bytes)?;
    }
    Ok(())
}
