//! Desktop identity and logo for standalone Linux executables.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) const APP_ID: &str = "virial-gpui";
const LOGO: &[u8] = include_bytes!("../../../assets/images/virial-gpui-logo.png");

pub(crate) fn register() -> io::Result<()> {
    let binary = env::current_exe()?;
    if system_registered(&binary, Path::new("/usr")) {
        return Ok(());
    }
    let data = data_home(
        env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    )?;
    if install(&data, &binary)? {
        refresh_desktop_cache(&data);
    }
    Ok(())
}

fn system_registered(binary: &Path, prefix: &Path) -> bool {
    binary == prefix.join("bin").join(APP_ID)
        && prefix
            .join(format!("share/applications/{APP_ID}.desktop"))
            .is_file()
        && prefix
            .join(format!("share/icons/hicolor/512x512/apps/{APP_ID}.png"))
            .is_file()
}

fn data_home(xdg: Option<PathBuf>, home: Option<PathBuf>) -> io::Result<PathBuf> {
    xdg.filter(|path| path.is_absolute())
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|home| home.join(".local/share"))
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No user data directory"))
}

fn value(path: &Path) -> io::Result<String> {
    let text = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Non-UTF-8 desktop path"))?;
    Ok(text
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t"))
}

fn executable(path: &Path) -> io::Result<String> {
    let text = path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Non-UTF-8 executable path"))?;
    // Exec quoting is decoded after desktop string escaping; percent signs
    // must also survive the launcher's field-code expansion.
    let quoted = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    Ok(format!("\"{}\"", value(Path::new(&quoted))?))
}

fn write_changed(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(true)
}

fn install(data: &Path, binary: &Path) -> io::Result<bool> {
    let icon = data.join(format!("icons/hicolor/512x512/apps/{APP_ID}.png"));
    // An absolute icon path also works before the icon theme cache refreshes.
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=Virial\nComment=Linux file manager\n\
         Exec={}\nIcon={}\nStartupWMClass={APP_ID}\nTerminal=false\n\
         Categories=System;FileTools;FileManager;\n",
        executable(binary)?,
        value(&icon)?,
    );
    let icon_changed = write_changed(&icon, LOGO)?;
    let launcher_changed = write_changed(
        &data.join(format!("applications/{APP_ID}.desktop")),
        desktop.as_bytes(),
    )?;
    if icon_changed || launcher_changed {
        // Theme lookups cache misses too. Updating only a nested PNG does not
        // invalidate those caches; the theme directory itself must change.
        fs::File::open(data.join("icons/hicolor"))?
            .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::now()))?;
    }
    Ok(icon_changed || launcher_changed)
}

fn refresh_desktop_cache(data: &Path) {
    // KDE's application menu caches desktop entries separately from icons.
    // These helpers are optional, and registration must work without them.
    for helper in ["kbuildsycoca6", "kbuildsycoca5"] {
        if let Ok(status) = Command::new(helper)
            .arg("--noincremental")
            .env("XDG_DATA_HOME", data)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            if status.success() {
                break;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/platform/linux/desktop.rs"]
mod tests;
