use crate::{
    app::FileManager,
    platform::linux::devices::{self, Action, Volume},
};
use gpui::Context;
use std::time::Duration;

impl FileManager {
    pub(crate) fn monitor_devices(&mut self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        self.device_monitor = Some(cx.spawn(async move |view, cx| {
            loop {
                let Ok(generation) = view.update(cx, |view, _| view.device_generation) else {
                    break;
                };
                let result = executor.spawn(async { devices::discover() }).await;
                if view
                    .update(cx, |view, cx| {
                        if view.busy || view.device_generation != generation {
                            return;
                        }
                        match result {
                            Ok(devices) => {
                                if view.device_error.take().is_some() {
                                    cx.notify();
                                }
                                view.update_devices(devices, cx);
                            }
                            Err(error) => {
                                let message = format!(
                                    "{}: {error}",
                                    view.language.text("Cannot read devices")
                                );
                                if view.device_error.as_ref() != Some(&message) {
                                    view.device_error = Some(message);
                                    cx.notify();
                                }
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
                executor.timer(Duration::from_secs(2)).await;
            }
        }));
    }

    fn update_devices(&mut self, devices: Vec<Volume>, cx: &mut Context<Self>) {
        let lost_location = self.location.directory().is_some_and(|directory| {
            self.devices
                .iter()
                .flat_map(|device| &device.mountpoints)
                .any(|mount| {
                    directory.starts_with(mount)
                        && !devices
                            .iter()
                            .any(|device| device.mountpoints.contains(mount))
                })
        });
        if lost_location {
            self.preview_task = None;
            self.preview_path = None;
            self.navigate(self.home.clone(), cx);
        }
        if self.devices != devices {
            self.devices = devices;
            cx.notify();
        }
    }

    pub(crate) fn device_action(&mut self, volume: Volume, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.dialog.is_some() {
            return;
        }
        if action == Action::Open {
            if let Some(mount) = volume.mountpoints.first() {
                self.navigate(mount.clone(), cx);
                return;
            }
        } else {
            self.directory_sizes = None;
            self.preview_task = None;
            self.global_search = None;
        }
        self.busy = true;
        self.error = None;
        self.device_generation = self.device_generation.wrapping_add(1);
        let navigation = self.navigation_generation;
        let task = cx.background_executor().spawn(async move {
            let result = devices::execute(&volume, action);
            // An operation can partially succeed (e.g. a sibling is busy).
            (result, devices::discover())
        });
        cx.spawn(async move |view, cx| {
            let (result, devices) = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                view.device_generation = view.device_generation.wrapping_add(1);
                if let Ok(devices) = devices {
                    view.update_devices(devices, cx);
                }
                match result {
                    Ok(Some(mount)) if view.navigation_generation == navigation => {
                        view.navigate(mount, cx)
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Device operation failed")
                        ))
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
