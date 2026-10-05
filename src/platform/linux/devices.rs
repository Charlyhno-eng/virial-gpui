//! Removable volumes and safe device operations through the UDisks2 D-Bus API.
use futures_lite::{StreamExt, future};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    time::Duration,
};
use zbus::{
    blocking::{Connection, Proxy},
    fdo::ManagedObjects,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const SERVICE: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
const PARTITION: &str = "org.freedesktop.UDisks2.Partition";
const ROOT: &str = "/org/freedesktop/UDisks2";
type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Volume {
    pub object: OwnedObjectPath,
    pub drive: OwnedObjectPath,
    pub device: PathBuf,
    pub label: String,
    pub size: u64,
    pub mountpoints: Vec<PathBuf>,
    pub can_power_off: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Open,
    Unmount,
    SafelyRemove,
}

fn connection() -> io::Result<Connection> {
    zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.method_timeout(Duration::from_secs(60)).build())
        .map_err(io::Error::other)
}

fn objects(connection: &Connection) -> io::Result<ManagedObjects> {
    let proxy = Proxy::new(
        connection,
        SERVICE,
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
    )
    .map_err(io::Error::other)?;
    proxy
        .call("GetManagedObjects", &())
        .map_err(io::Error::other)
}

pub(crate) fn discover() -> io::Result<Vec<Volume>> {
    Ok(volumes(&objects(&connection()?)?))
}

// Subscribe before the first snapshot so hotplug events during discovery are
// retained. One connection sleeps on signals instead of reconnecting every 2 s.
pub(crate) struct Monitor {
    connection: zbus::Connection,
    changes: zbus::MessageStream,
    owners: zbus::MessageStream,
}

impl Monitor {
    pub(crate) async fn new() -> io::Result<Self> {
        let connection = zbus::connection::Builder::system()
            .map_err(io::Error::other)?
            .method_timeout(Duration::from_secs(60))
            .build()
            .await
            .map_err(io::Error::other)?;
        Self::subscribe(connection).await
    }

    async fn subscribe(connection: zbus::Connection) -> io::Result<Self> {
        let changes = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(SERVICE)
            .and_then(|rule| rule.path_namespace(ROOT))
            .map_err(io::Error::other)?
            .build();
        let owners = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .and_then(|rule| rule.interface("org.freedesktop.DBus"))
            .and_then(|rule| rule.member("NameOwnerChanged"))
            .and_then(|rule| rule.add_arg(SERVICE))
            .map_err(io::Error::other)?
            .build();
        Ok(Self {
            changes: zbus::MessageStream::for_match_rule(changes, &connection, None)
                .await
                .map_err(io::Error::other)?,
            owners: zbus::MessageStream::for_match_rule(owners, &connection, Some(8))
                .await
                .map_err(io::Error::other)?,
            connection,
        })
    }

    pub(crate) async fn discover(&mut self) -> io::Result<Vec<Volume>> {
        let proxy = zbus::Proxy::new(
            &self.connection,
            SERVICE,
            ROOT,
            "org.freedesktop.DBus.ObjectManager",
        )
        .await
        .map_err(io::Error::other)?;
        loop {
            let mut changed = false;
            // Drain signals during the call: a hotplug burst must not fill the
            // bounded signal queue and block delivery of the method reply.
            let snapshot = async {
                proxy
                    .call::<_, _, ManagedObjects>("GetManagedObjects", &())
                    .await
                    .map_err(io::Error::other)
            };
            let changes = async {
                loop {
                    wait_for_change(&mut self.changes, &mut self.owners).await?;
                    changed = true;
                }
            };
            let objects = future::race(snapshot, changes).await?;
            if !changed {
                return Ok(volumes(&objects));
            }
            // A signal may have arrived after the server took its snapshot.
        }
    }

    pub(crate) async fn changed(&mut self) -> io::Result<()> {
        wait_for_change(&mut self.changes, &mut self.owners).await
    }
}

async fn wait_for_change(
    changes: &mut zbus::MessageStream,
    owners: &mut zbus::MessageStream,
) -> io::Result<()> {
    loop {
        let message = future::race(changes.next(), owners.next())
            .await
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "Device monitor closed"))?
            .map_err(io::Error::other)?;
        if affects_volumes(&message) {
            return Ok(());
        }
    }
}

fn affects_volumes(message: &zbus::Message) -> bool {
    let header = message.header();
    match (
        header.interface().map(|name| name.as_str()),
        header.member().map(|name| name.as_str()),
    ) {
        (Some("org.freedesktop.DBus"), Some("NameOwnerChanged"))
        | (
            Some("org.freedesktop.DBus.ObjectManager"),
            Some("InterfacesAdded" | "InterfacesRemoved"),
        ) => true,
        (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
            let Ok((interface, changed, invalidated)) =
                message
                    .body()
                    .deserialize::<(String, Properties, Vec<String>)>()
            else {
                return true;
            };
            // UDisks also publishes temperature/SMART statistics. Those do not
            // affect the sidebar and must not wake filesystem discovery.
            let relevant: &[&str] = match interface.as_str() {
                BLOCK => &[
                    "Device",
                    "Drive",
                    "IdLabel",
                    "Size",
                    "HintIgnore",
                    "HintSystem",
                ],
                DRIVE => &[
                    "Removable",
                    "MediaRemovable",
                    "ConnectionBus",
                    "Model",
                    "CanPowerOff",
                ],
                FILESYSTEM => &["MountPoints"],
                PARTITION => &["Table"],
                _ => return false,
            };
            relevant.iter().any(|name| {
                changed.contains_key(*name) || invalidated.iter().any(|item| item == name)
            })
        }
        _ => false,
    }
}

fn text<'a>(properties: &'a Properties, name: &str) -> Option<&'a str> {
    properties
        .get(name)
        .and_then(|value| <&str>::try_from(value).ok())
}

fn flag(properties: &Properties, name: &str) -> bool {
    properties
        .get(name)
        .and_then(|value| bool::try_from(value).ok())
        .unwrap_or(false)
}

fn object_path(properties: &Properties, name: &str) -> Option<OwnedObjectPath> {
    properties.get(name)?.try_clone().ok()?.try_into().ok()
}

fn path(mut bytes: Vec<u8>) -> Option<PathBuf> {
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    let path = PathBuf::from(OsString::from_vec(bytes));
    path.is_absolute().then_some(path)
}

fn mountpoints(properties: &Properties) -> Vec<PathBuf> {
    properties
        .get("MountPoints")
        .and_then(|value| Vec::<Vec<u8>>::try_from(value.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(path)
        .collect()
}

fn system_mount(path: &Path) -> bool {
    path == Path::new("/")
        || ["/boot", "/home", "/usr", "/var", "/etc", "/opt"]
            .iter()
            .any(|root| path.starts_with(root))
}

fn volumes(objects: &ManagedObjects) -> Vec<Volume> {
    // A hybrid installation image can expose a filesystem on both the disk and
    // its partitions. Show only the partitions in that case.
    let partitioned: HashSet<_> = objects
        .values()
        .filter_map(|interfaces| object_path(interfaces.get(PARTITION)?, "Table"))
        .collect();
    // Never offer removal of a drive containing a mounted system filesystem.
    let system_drives: HashSet<_> = objects
        .values()
        .filter_map(|interfaces| {
            let block = interfaces.get(BLOCK)?;
            let filesystem = interfaces.get(FILESYSTEM)?;
            mountpoints(filesystem)
                .iter()
                .any(|mount| system_mount(mount))
                .then(|| object_path(block, "Drive"))
                .flatten()
        })
        .collect();
    let mut result = Vec::new();
    for (object, interfaces) in objects {
        let Some(block) = interfaces.get(BLOCK) else {
            continue;
        };
        let Some(filesystem) = interfaces.get(FILESYSTEM) else {
            continue;
        };
        let Some(drive) = object_path(block, "Drive") else {
            continue;
        };
        let Some(properties) = objects
            .get(&drive)
            .and_then(|interfaces| interfaces.get(DRIVE))
        else {
            continue;
        };
        if flag(block, "HintIgnore")
            || flag(block, "HintSystem")
            || system_drives.contains(&drive)
            || partitioned.contains(object)
            || !(flag(properties, "Removable")
                || flag(properties, "MediaRemovable")
                || text(properties, "ConnectionBus") == Some("usb"))
        {
            continue;
        }
        let Some(device) = block
            .get("Device")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
            .and_then(path)
        else {
            continue;
        };
        let label = text(block, "IdLabel")
            .filter(|label| !label.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let model = text(properties, "Model").unwrap_or("").trim();
                if model.is_empty() {
                    device.display().to_string()
                } else {
                    format!(
                        "{model} ({})",
                        device.file_name().unwrap_or_default().to_string_lossy()
                    )
                }
            });
        result.push(Volume {
            object: object.clone(),
            drive,
            device,
            label,
            size: block
                .get("Size")
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(0),
            mountpoints: mountpoints(filesystem),
            can_power_off: flag(properties, "CanPowerOff"),
        });
    }
    result.sort_by(|a, b| a.device.cmp(&b.device));
    result
}

// Keeping the plan separate makes ordering and failure handling testable without
// mounting or disconnecting a developer's real devices.
fn removal_plan<'a>(
    volume: &'a Volume,
    current: &'a [Volume],
    action: Action,
) -> Vec<(&'a OwnedObjectPath, &'static str, &'static str)> {
    let mut plan = Vec::new();
    for sibling in current {
        if (action == Action::SafelyRemove && sibling.drive == volume.drive
            || action == Action::Unmount && sibling.object == volume.object)
            && !sibling.mountpoints.is_empty()
        {
            plan.push((&sibling.object, FILESYSTEM, "Unmount"));
        }
    }
    if action == Action::SafelyRemove {
        plan.push((&volume.drive, DRIVE, "PowerOff"));
    }
    plan
}

pub(crate) fn execute(volume: &Volume, action: Action) -> io::Result<Option<PathBuf>> {
    let connection = connection()?;
    // Recheck the live objects: unplugging or a mount in another application may
    // have changed the device since the sidebar was rendered.
    let current = volumes(&objects(&connection)?);
    let volume = current
        .iter()
        .find(|item| item.object == volume.object && item.drive == volume.drive)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Device is no longer available"))?;
    let options = HashMap::<&str, Value<'_>>::new();
    if action == Action::Open {
        if let Some(mount) = volume.mountpoints.first() {
            return Ok(Some(mount.clone()));
        }
        let proxy = Proxy::new(&connection, SERVICE, volume.object.as_str(), FILESYSTEM)
            .map_err(io::Error::other)?;
        let mounted: String = proxy.call("Mount", &(options,)).map_err(io::Error::other)?;
        return Ok(Some(PathBuf::from(mounted)));
    }
    if action == Action::SafelyRemove && !volume.can_power_off {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "This drive does not support safe power-off; unmount its volumes instead",
        ));
    }
    for (object, interface, method) in removal_plan(volume, &current, action) {
        let proxy = Proxy::new(&connection, SERVICE, object.as_str(), interface)
            .map_err(io::Error::other)?;
        // Never force an unmount. Stop immediately if any volume is in use.
        proxy
            .call::<_, _, ()>(method, &(&options,))
            .map_err(io::Error::other)?;
    }
    Ok(None)
}

#[cfg(test)]
#[path = "../../../tests/platform/linux/devices.rs"]
mod tests;
