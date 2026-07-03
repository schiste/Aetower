//! `aetower install` / `aetower uninstall` — manage the PATH symlink.
//!
//! The binary itself ships inside the app bundle (`Contents/Helpers/aetower`),
//! which is why it arrives through every channel — pkg, dmg, zip, brew. Getting
//! `aetower` onto `$PATH` is a separate, explicit step: a symlink in
//! `/usr/local/bin`. The flagship PKG does this in its postinstall; DMG/ZIP/brew
//! users (or anyone who wants it) run `aetower install`.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

const LINK_DIR: &str = "/usr/local/bin";
const LINK_NAME: &str = "aetower";

fn link_path() -> PathBuf {
    Path::new(LINK_DIR).join(LINK_NAME)
}

/// Create (or refresh) `/usr/local/bin/aetower` pointing at this executable.
pub fn install() -> Result<String, String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("locate current executable: {error}"))?;
    let link = link_path();

    if let Some(parent) = link.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }

    match std::fs::symlink_metadata(&link) {
        Ok(meta) => {
            if meta.file_type().is_symlink()
                && let Ok(existing) = std::fs::read_link(&link)
                && existing == exe
            {
                return Ok(format!(
                    "already installed: {} → {}",
                    link.display(),
                    exe.display()
                ));
            }
            std::fs::remove_file(&link)
                .map_err(|error| format!("replace {}: {error}", link.display()))?;
        }
        Err(ref error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("inspect {}: {error}", link.display())),
    }

    symlink(&exe, &link).map_err(|error| {
        format!(
            "symlink {} → {}: {error}\n\
             If {LINK_DIR} needs admin rights, run:\n  sudo ln -sf \"{}\" \"{}\"",
            link.display(),
            exe.display(),
            exe.display(),
            link.display()
        )
    })?;
    Ok(format!("installed: {} → {}", link.display(), exe.display()))
}

/// Remove the PATH symlink if (and only if) it is one we created.
pub fn uninstall() -> Result<String, String> {
    let link = link_path();
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(&link)
                .map_err(|error| format!("remove {}: {error}", link.display()))?;
            Ok(format!("removed {}", link.display()))
        }
        Ok(_) => Err(format!(
            "{} exists but is not an aetower symlink; leaving it untouched",
            link.display()
        )),
        Err(ref error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(format!("not installed ({} absent)", link.display()))
        }
        Err(error) => Err(format!("inspect {}: {error}", link.display())),
    }
}
