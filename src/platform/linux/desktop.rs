//! Desktop identity and logo for standalone Linux executables.
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub(crate) const APP_ID: &str = "virial-gpui";
const LOGO: &[u8] = include_bytes!("../../../assets/images/virial-gpui-logo.png");

pub(crate) fn register() -> io::Result<()> {
    let data = data_home(
        env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    )?;
    install(&data, &env::current_exe()?)
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

fn write_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == bytes => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

fn install(data: &Path, binary: &Path) -> io::Result<()> {
    let icon = data.join(format!("icons/hicolor/512x512/apps/{APP_ID}.png"));
    // An absolute icon path also works before the icon theme cache refreshes.
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=Virial\nComment=Linux file manager\n\
         Exec={}\nIcon={}\nStartupWMClass={APP_ID}\nTerminal=false\n\
         Categories=System;FileTools;FileManager;\n",
        executable(binary)?,
        value(&icon)?,
    );
    write_changed(&icon, LOGO)?;
    write_changed(
        &data.join(format!("applications/{APP_ID}.desktop")),
        desktop.as_bytes(),
    )
}

#[cfg(test)]
#[path = "../../../tests/platform/linux/desktop.rs"]
mod tests;
