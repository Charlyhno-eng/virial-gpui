use super::*;
use zbus::zvariant::ObjectPath;

fn value(value: impl Into<Value<'static>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).unwrap()
}

fn fixture() -> ManagedObjects {
    let drive = "/org/freedesktop/UDisks2/drives/USB";
    let disk = "/org/freedesktop/UDisks2/block_devices/sda";
    let mut objects = ManagedObjects::new();
    objects.insert(
        drive.try_into().unwrap(),
        HashMap::from([(
            DRIVE.try_into().unwrap(),
            HashMap::from([
                ("ConnectionBus".into(), value("usb")),
                ("Model".into(), value("SanDisk")),
                ("CanPowerOff".into(), value(true)),
            ]),
        )]),
    );
    for (index, label, mounts) in [
        ("", "Linux Mint", vec![]),
        (
            "1",
            "Linux Mint",
            vec![b"/media/user/Linux Mint\0".to_vec()],
        ),
        ("2", "", vec![]),
        ("3", "writable", vec![b"/media/user/writable\0".to_vec()]),
    ] {
        let object = format!("{disk}{index}");
        let mut interfaces = HashMap::from([
            (
                BLOCK.try_into().unwrap(),
                HashMap::from([
                    (
                        "Device".into(),
                        value(format!("/dev/sda{index}\0").into_bytes()),
                    ),
                    ("Drive".into(), value(ObjectPath::try_from(drive).unwrap())),
                    ("IdLabel".into(), value(label)),
                    ("Size".into(), value(4096u64)),
                ]),
            ),
            (
                FILESYSTEM.try_into().unwrap(),
                HashMap::from([("MountPoints".into(), value(mounts))]),
            ),
        ]);
        if !index.is_empty() {
            interfaces.insert(
                PARTITION.try_into().unwrap(),
                HashMap::from([("Table".into(), value(ObjectPath::try_from(disk).unwrap()))]),
            );
        }
        objects.insert(object.try_into().unwrap(), interfaces);
    }
    objects
}

#[test]
fn discovers_usb_partitions_without_duplicate_hybrid_disk() {
    let found = volumes(&fixture());
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].label, "Linux Mint");
    assert_eq!(
        found[0].mountpoints,
        vec![PathBuf::from("/media/user/Linux Mint")]
    );
    assert_eq!(found[1].label, "SanDisk (sda2)");
    assert!(found[1].mountpoints.is_empty());
    assert_eq!(found[2].label, "writable");
    assert!(found.iter().all(|volume| volume.can_power_off));
}

#[test]
fn excludes_internal_ignored_and_system_devices() {
    let mut objects = fixture();
    let drive = objects
        .values_mut()
        .find_map(|interfaces| interfaces.get_mut(DRIVE))
        .unwrap();
    drive.insert("ConnectionBus".into(), value("nvme"));
    assert!(volumes(&objects).is_empty());

    for hint in ["HintIgnore", "HintSystem"] {
        let mut objects = fixture();
        let block = objects
            .get_mut(&ObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sda1").unwrap())
            .unwrap()
            .get_mut(BLOCK)
            .unwrap();
        block.insert(hint.into(), value(true));
        assert_eq!(volumes(&objects).len(), 2);
    }
    let mut objects = fixture();
    let fs = objects
        .get_mut(&ObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sda3").unwrap())
        .unwrap()
        .get_mut(FILESYSTEM)
        .unwrap();
    fs.insert("MountPoints".into(), value(vec![b"/\0".to_vec()]));
    assert!(volumes(&objects).is_empty());
}

#[test]
fn supports_whole_disk_filesystems_and_non_utf8_mount_paths() {
    let mut objects = fixture();
    objects.retain(|_, interfaces| !interfaces.contains_key(PARTITION));
    let fs = objects
        .get_mut(&ObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sda").unwrap())
        .unwrap()
        .get_mut(FILESYSTEM)
        .unwrap();
    fs.insert(
        "MountPoints".into(),
        value(vec![b"/media/user/\xff\0".to_vec()]),
    );
    let found = volumes(&objects);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].mountpoints[0],
        PathBuf::from(OsString::from_vec(b"/media/user/\xff".to_vec()))
    );
}

#[test]
fn safe_removal_unmounts_all_mounted_siblings_before_power_off() {
    let found = volumes(&fixture());
    let plan = removal_plan(&found[1], &found, Action::SafelyRemove);
    assert_eq!(
        plan,
        vec![
            (&found[0].object, FILESYSTEM, "Unmount"),
            (&found[2].object, FILESYSTEM, "Unmount"),
            (&found[1].drive, DRIVE, "PowerOff"),
        ]
    );
    assert_eq!(
        removal_plan(&found[0], &found, Action::Unmount),
        vec![(&found[0].object, FILESYSTEM, "Unmount")]
    );
    assert!(removal_plan(&found[1], &found, Action::Unmount).is_empty());
}

fn properties_signal(
    interface: &str,
    changed: Properties,
    invalidated: Vec<String>,
) -> zbus::Message {
    zbus::Message::signal(
        "/org/freedesktop/UDisks2/block_devices/sda1",
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
    )
    .unwrap()
    .build(&(interface, changed, invalidated))
    .unwrap()
}

#[test]
fn device_notifications_include_invalidations_and_ignore_unrelated_statistics() {
    for (interface, names) in [
        (
            BLOCK,
            vec![
                "Device",
                "Drive",
                "IdLabel",
                "Size",
                "HintIgnore",
                "HintSystem",
            ],
        ),
        (
            DRIVE,
            vec![
                "Removable",
                "MediaRemovable",
                "ConnectionBus",
                "Model",
                "CanPowerOff",
            ],
        ),
        (FILESYSTEM, vec!["MountPoints"]),
        (PARTITION, vec!["Table"]),
    ] {
        for name in names {
            assert!(affects_volumes(&properties_signal(
                interface,
                HashMap::from([(name.into(), value(true))]),
                Vec::new(),
            )));
            assert!(affects_volumes(&properties_signal(
                interface,
                HashMap::new(),
                vec![name.into()],
            )));
        }
    }
    assert!(!affects_volumes(&properties_signal(
        "org.freedesktop.UDisks2.Drive.Ata",
        HashMap::from([("SmartTemperature".into(), value(300u64))]),
        Vec::new(),
    )));
    assert!(!affects_volumes(&properties_signal(
        BLOCK,
        HashMap::from([("UnusedProperty".into(), value(true))]),
        Vec::new(),
    )));
    assert!(!affects_volumes(&properties_signal(
        FILESYSTEM,
        HashMap::new(),
        Vec::new()
    )));
}

struct TestObjects {
    objects: std::sync::Arc<std::sync::Mutex<ManagedObjects>>,
    requests: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[zbus::interface(name = "org.freedesktop.DBus.ObjectManager")]
impl TestObjects {
    fn get_managed_objects(&self) -> ManagedObjects {
        self.requests
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.objects.lock().unwrap().clone()
    }
}

struct TestBus(std::process::Child);

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn device_monitor_sleeps_until_changes_and_survives_service_restart() {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
    };

    // An isolated bus exercises real signal routing without touching any devices.
    let child = match Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("Skipping isolated device monitor check: dbus-daemon unavailable");
            return;
        }
        Err(error) => panic!("Start isolated bus: {error}"),
    };
    let mut bus = TestBus(child);
    let mut address = String::new();
    std::io::BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let (done, completion) = mpsc::channel();
    std::thread::spawn(move || {
        future::block_on(async {
            let objects = Arc::new(Mutex::new(fixture()));
            let requests = Arc::new(AtomicUsize::new(0));
            let server = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .name(SERVICE)
                .unwrap()
                .serve_at(
                    ROOT,
                    TestObjects {
                        objects: objects.clone(),
                        requests: requests.clone(),
                    },
                )
                .unwrap()
                .build()
                .await
                .unwrap();
            let client = zbus::connection::Builder::address(address.trim())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .build()
                .await
                .unwrap();
            let mut monitor = Monitor::subscribe(client.clone()).await.unwrap();
            assert_eq!(monitor.discover().await.unwrap(), volumes(&fixture()));
            let initial_requests = requests.load(Ordering::Relaxed);
            assert!(future::poll_once(monitor.changed()).await.is_none());
            assert_eq!(requests.load(Ordering::Relaxed), initial_requests);

            // Queue more signals than the monitor's bounded queue can hold, then
            // discover. Snapshot calls must drain the burst rather than deadlock.
            let object = "/org/freedesktop/UDisks2/block_devices/sda1";
            let mounts = vec![b"/media/user/new\0".to_vec()];
            objects
                .lock()
                .unwrap()
                .get_mut(&ObjectPath::try_from(object).unwrap())
                .unwrap()
                .get_mut(FILESYSTEM)
                .unwrap()
                .insert("MountPoints".into(), value(mounts.clone()));
            for _ in 0..100 {
                server
                    .emit_signal(
                        None::<&str>,
                        object,
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &(
                            FILESYSTEM,
                            HashMap::from([("MountPoints", value(mounts.clone()))]),
                            Vec::<String>::new(),
                        ),
                    )
                    .await
                    .unwrap();
            }
            monitor.changed().await.unwrap();
            assert_eq!(
                monitor.discover().await.unwrap()[0].mountpoints,
                vec![PathBuf::from("/media/user/new")]
            );

            objects
                .lock()
                .unwrap()
                .remove(&ObjectPath::try_from(object).unwrap());
            server
                .emit_signal(
                    None::<&str>,
                    ROOT,
                    "org.freedesktop.DBus.ObjectManager",
                    "InterfacesRemoved",
                    &(ObjectPath::try_from(object).unwrap(), vec![FILESYSTEM]),
                )
                .await
                .unwrap();
            monitor.changed().await.unwrap();
            assert_eq!(monitor.discover().await.unwrap().len(), 2);

            server.release_name(SERVICE).await.unwrap();
            monitor.changed().await.unwrap();
            assert!(monitor.discover().await.is_err());
            server.request_name(SERVICE).await.unwrap();
            monitor.changed().await.unwrap();
            assert_eq!(monitor.discover().await.unwrap().len(), 2);

            client.close().await.unwrap();
            assert!(monitor.changed().await.is_err());
        });
        done.send(()).unwrap();
    });
    completion
        .recv_timeout(Duration::from_secs(15))
        .expect("Device monitor stalled or failed");
}

#[test]
#[ignore = "Requires a desktop UDisks2 service; read-only check of connected devices"]
fn connected_devices_can_be_discovered_and_browsed() {
    let found = discover().unwrap();
    assert!(!found.is_empty(), "Connect a removable drive first");
    for volume in &found {
        println!(
            "{}: {} ({:?})",
            volume.device.display(),
            volume.label,
            volume.mountpoints
        );
        for mount in &volume.mountpoints {
            assert!(
                std::fs::read_dir(mount).is_ok(),
                "Cannot browse {}",
                mount.display()
            );
        }
    }
}

#[test]
#[ignore = "Explicit USB integration check: unmounts and remounts VIRIAL_TEST_USB_DEVICE"]
fn usb_volume_can_be_unmounted_and_remounted() {
    let device = PathBuf::from(
        std::env::var_os("VIRIAL_TEST_USB_DEVICE")
            .expect("Set VIRIAL_TEST_USB_DEVICE to the test partition"),
    );
    let found = discover().unwrap();
    let volume = found
        .iter()
        .find(|volume| volume.device == device)
        .expect("Test partition is not removable");
    assert!(
        !volume.mountpoints.is_empty(),
        "Test partition must initially be mounted"
    );
    let held = std::fs::File::open(&volume.mountpoints[0]).unwrap();
    let busy_result = execute(volume, Action::Unmount);
    drop(held);
    assert!(
        busy_result.is_err(),
        "An open volume must not be forcibly unmounted"
    );
    execute(volume, Action::Unmount).unwrap();
    // Always attempt remount before assertions so a discovery failure cannot
    // unnecessarily leave the developer's volume unmounted.
    let unmounted = discover();
    let mount = execute(volume, Action::Open)
        .expect("Remount test partition")
        .unwrap();
    assert!(
        unmounted
            .unwrap()
            .iter()
            .find(|volume| volume.device == device)
            .unwrap()
            .mountpoints
            .is_empty()
    );
    assert!(std::fs::read_dir(&mount).is_ok());
    assert!(
        discover()
            .unwrap()
            .iter()
            .find(|volume| volume.device == device)
            .unwrap()
            .mountpoints
            .contains(&mount)
    );
    println!(
        "Unmounted, remounted, and browsed {} at {}",
        device.display(),
        mount.display()
    );
}
