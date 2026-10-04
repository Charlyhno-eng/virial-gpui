use super::*;
#[test]
fn discovers_gvfs_and_network_mounts_but_not_local_disks() {
    let root = std::env::temp_dir().join(format!("virial-network-test-{}", std::process::id()));
    fs::create_dir_all(root.join("gvfs/smb-share:server=nas,share=docs")).unwrap();
    fs::write(root.join("gvfs/not-a-share"), "").unwrap();
    let mount = root.join("shared files");
    fs::create_dir(&mount).unwrap();
    let escaped = mount.to_string_lossy().replace(' ', "\\040");
    let mounts = format!(
        "1 2 0:1 / {escaped} rw - cifs //nas/docs rw\n2 3 0:2 / / rw - ext4 /dev/sda rw\n3 4 0:3 / {escaped} rw - nfs nas:/docs rw"
    );
    assert_eq!(mount_paths(&mounts), vec![mount.clone(), mount.clone()]);
    let entries = read_from(&root, &mounts).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| entry.path == mount));
    assert!(read_from(&root.join("absent"), "").unwrap().is_empty());
    fs::remove_dir_all(root).unwrap();
}
