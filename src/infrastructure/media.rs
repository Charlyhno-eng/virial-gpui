//! Optional libmpv playback. The UI only reads snapshots and queues commands.
use gpui::RenderImage;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::PathBuf,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub(crate) struct Snapshot {
    pub frame: Option<Arc<RenderImage>>,
    pub ready: bool,
    pub failed: bool,
    pub paused: bool,
    pub muted: bool,
    pub position: f64,
    pub duration: f64,
}

impl Snapshot {
    pub(crate) fn changed_since(&self, previous: &Self) -> bool {
        self.frame.as_ref().map(|frame| frame.id) != previous.frame.as_ref().map(|frame| frame.id)
            || self.ready != previous.ready || self.failed != previous.failed
            || self.paused != previous.paused || self.muted != previous.muted
            // The controls display whole seconds, so audio needs no frame-rate redraw.
            || self.position as u64 != previous.position as u64
            || self.duration as u64 != previous.duration as u64
    }
}

pub(crate) enum Control {
    Pause(bool),
    Seek(f64),
    Restart,
    Mute(bool),
}

pub(crate) struct Media {
    state: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    commands: mpsc::Sender<Control>,
    pub video: bool,
}

impl Media {
    pub fn new(path: PathBuf, video: bool, archived: Option<super::archive::Materialized>) -> Self {
        let state = Arc::new(Mutex::new(Snapshot {
            paused: true,
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, receiver) = mpsc::channel();
        let worker_state = state.clone();
        let worker_stop = stop.clone();
        std::thread::spawn(move || {
            // Keep a ZIP member's temporary copy alive until playback has stopped.
            let _archived = archived;
            if run(&path, video, &worker_state, &worker_stop, receiver).is_err() {
                worker_state.lock().unwrap().failed = true;
            }
        });
        Self {
            state,
            stop,
            commands,
            video,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }

    pub fn control(&self, command: Control) {
        let _ = self.commands.send(command);
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        // Destruction and decoder shutdown happen off the UI thread.
        self.stop.store(true, Ordering::Relaxed);
    }
}

// Minimal stable C API declarations, matching include/mpv/{client,render}.h.
// The library stays loaded until both the control and rendering threads exit.
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
impl Param {
    fn new(kind: c_int, data: *mut c_void) -> Self {
        Self { kind, data }
    }
    fn end() -> Self {
        Self::new(0, ptr::null_mut())
    }
}
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

struct Api {
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    destroy: unsafe extern "C" fn(*mut c_void),
    option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
    property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    wait: unsafe extern "C" fn(*mut c_void, f64) -> *const Event,
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut Param) -> c_int,
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render: unsafe extern "C" fn(*mut c_void, *mut Param) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
    _library: libloading::Library,
}

impl Api {
    fn load() -> Result<Arc<Self>, ()> {
        // SAFETY: symbols use the public libmpv C ABI; all copied function pointers
        // are kept alive by _library. No user-provided libraries are loaded.
        unsafe {
            #[cfg(unix)]
            let library = libloading::Library::new("libmpv.so.2").map_err(|_| ())?;
            #[cfg(windows)]
            let library = libloading::Library::new("libmpv-2.dll").map_err(|_| ())?;
            Ok(Arc::new(Self {
                create: *library.get(b"mpv_create\0").map_err(|_| ())?,
                initialize: *library.get(b"mpv_initialize\0").map_err(|_| ())?,
                destroy: *library.get(b"mpv_terminate_destroy\0").map_err(|_| ())?,
                option: *library.get(b"mpv_set_option_string\0").map_err(|_| ())?,
                command: *library.get(b"mpv_command\0").map_err(|_| ())?,
                property: *library.get(b"mpv_get_property\0").map_err(|_| ())?,
                wait: *library.get(b"mpv_wait_event\0").map_err(|_| ())?,
                render_create: *library
                    .get(b"mpv_render_context_create\0")
                    .map_err(|_| ())?,
                render_update: *library
                    .get(b"mpv_render_context_update\0")
                    .map_err(|_| ())?,
                render: *library
                    .get(b"mpv_render_context_render\0")
                    .map_err(|_| ())?,
                render_free: *library.get(b"mpv_render_context_free\0").map_err(|_| ())?,
                _library: library,
            }))
        }
    }

    fn command(&self, handle: *mut c_void, args: &[&CStr]) -> Result<(), ()> {
        let mut pointers: Vec<_> = args.iter().map(|arg| arg.as_ptr()).collect();
        pointers.push(ptr::null());
        // SAFETY: handle is live; strings and the null-terminated array live through the call.
        if unsafe { (self.command)(handle, pointers.as_ptr()) } < 0 {
            Err(())
        } else {
            Ok(())
        }
    }

    fn number(&self, handle: *mut c_void, name: &CStr) -> f64 {
        let mut value = 0f64;
        // SAFETY: MPV_FORMAT_DOUBLE (5) writes exactly one f64.
        unsafe {
            (self.property)(handle, name.as_ptr(), 5, (&mut value as *mut f64).cast());
        }
        if value.is_finite() { value.max(0.) } else { 0. }
    }

    fn flag(&self, handle: *mut c_void, name: &CStr) -> bool {
        let mut value: c_int = 0;
        // SAFETY: MPV_FORMAT_FLAG (3) writes exactly one C int.
        unsafe {
            (self.property)(handle, name.as_ptr(), 3, (&mut value as *mut c_int).cast());
        }
        value != 0
    }
}

struct Player {
    api: Arc<Api>,
    handle: *mut c_void,
}
impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: the render thread was joined before this owner is dropped.
        unsafe {
            (self.api.destroy)(self.handle);
        }
    }
}

// Leave `ao` unset so mpv probes the available system audio outputs.
// `auto` is an audio-device value, not an audio output driver.
fn configure(api: &Api, handle: *mut c_void, video: bool) -> Result<(), ()> {
    for (name, value) in [
        (c"config", c"no"),
        (c"load-scripts", c"no"),
        (c"terminal", c"no"),
        (c"pause", c"yes"),
        (c"keep-open", c"yes"),
        (c"idle", c"yes"),
        (c"vo", if video { c"libmpv" } else { c"null" }),
        (c"audio-display", c"no"),
        (c"osc", c"no"),
        (c"osd-level", c"0"),
    ] {
        // SAFETY: all options are nul-terminated and handle is not yet initialized.
        if unsafe { (api.option)(handle, name.as_ptr(), value.as_ptr()) } < 0 {
            eprintln!("Failed mpv option: {name:?}={value:?}");
            return Err(());
        }
    }
    Ok(())
}

fn run(
    path: &std::path::Path,
    video: bool,
    state: &Arc<Mutex<Snapshot>>,
    stop: &Arc<AtomicBool>,
    commands: mpsc::Receiver<Control>,
) -> Result<(), ()> {
    let api = Api::load()?;
    #[cfg(unix)]
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| ())?;
    #[cfg(windows)]
    let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| ())?;
    // SAFETY: creates a uniquely owned handle, used only by this control thread.
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return Err(());
    }
    let player = Player {
        api: api.clone(),
        handle,
    };
    configure(&api, handle, video)?;
    // Integration tests decode without requiring a sound server or hardware.
    #[cfg(test)]
    if unsafe { (api.option)(handle, c"ao".as_ptr(), c"null".as_ptr()) } < 0 {
        return Err(());
    }
    if unsafe { (api.initialize)(handle) } < 0 {
        return Err(());
    }
    let renderer = if video {
        let mut context = ptr::null_mut();
        let mut params = [
            Param::new(1, c"sw".as_ptr().cast_mut().cast()),
            Param::end(),
        ];
        // SAFETY: context output and parameters match the software render ABI.
        if unsafe { (api.render_create)(&mut context, handle, params.as_mut_ptr()) } < 0 {
            return Err(());
        }
        let address = context as usize;
        let api = api.clone();
        let state = state.clone();
        let stop = stop.clone();
        Some(std::thread::spawn(move || {
            render_frames(api, address, state, stop)
        }))
    } else {
        None
    };
    let result = (|| {
        api.command(handle, &[c"loadfile", &path])?;
        let start = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            for command in commands.try_iter() {
                match command {
                    Control::Pause(paused) => {
                        if !paused && api.flag(handle, c"eof-reached") {
                            api.command(handle, &[c"seek", c"0", c"absolute"])?;
                        }
                        api.command(
                            handle,
                            &[c"set", c"pause", if paused { c"yes" } else { c"no" }],
                        )?;
                    }
                    Control::Mute(muted) => api.command(
                        handle,
                        &[c"set", c"mute", if muted { c"yes" } else { c"no" }],
                    )?,
                    Control::Seek(seconds) => {
                        let seconds = CString::new(seconds.to_string()).map_err(|_| ())?;
                        api.command(handle, &[c"seek", &seconds, c"relative"])?;
                    }
                    Control::Restart => api.command(handle, &[c"seek", c"0", c"absolute"])?,
                }
            }
            // SAFETY: event belongs to this handle and is read before the next wait.
            let event = unsafe { &*(api.wait)(handle, 0.1) };
            if event.id == 7 && !event.data.is_null() {
                let end = unsafe { &*event.data.cast::<EndFile>() };
                if end.error < 0 || end.reason == 4 {
                    eprintln!("Failed mpv file: {} {}", end.reason, end.error);
                    return Err(());
                }
            }
            if event.id == 1 {
                return Err(());
            }
            let ready = event.id == 8 || state.lock().unwrap().ready;
            if !ready && start.elapsed() > Duration::from_secs(10) {
                return Err(());
            }
            let position = api.number(handle, c"time-pos");
            let duration = api.number(handle, c"duration");
            let paused = api.flag(handle, c"pause");
            let muted = api.flag(handle, c"mute");
            let mut snapshot = state.lock().unwrap();
            snapshot.ready = ready;
            snapshot.position = position;
            snapshot.duration = duration;
            snapshot.paused = paused;
            snapshot.muted = muted;
        }
        Ok(())
    })();
    stop.store(true, Ordering::Relaxed);
    if let Some(renderer) = renderer {
        let _ = renderer.join();
    }
    drop(player);
    result
}

fn render_frames(
    api: Arc<Api>,
    address: usize,
    state: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
) {
    let context = address as *mut c_void;
    // Bound CPU rendering and memory use even for high-resolution source videos.
    const WIDTH: u32 = 960;
    const HEIGHT: u32 = 540;
    // 64-byte aligned storage and row stride for libmpv's software scaler.
    #[repr(align(64))]
    #[derive(Clone)]
    struct Block([u8; 64]);
    let mut pixels = vec![Block([0; 64]); (WIDTH * HEIGHT * 4 / 64) as usize];
    let mut size = [WIDTH as c_int, HEIGHT as c_int];
    let mut stride = (WIDTH * 4) as usize;
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: only this thread accesses the render context, which stays alive
        // until it frees it below. No control API calls occur on this thread.
        if unsafe { (api.render_update)(context) } & 1 != 0 {
            let mut params = [
                Param::new(17, size.as_mut_ptr().cast()),
                Param::new(18, c"bgr0".as_ptr().cast_mut().cast()),
                Param::new(19, (&mut stride as *mut usize).cast()),
                Param::new(20, pixels.as_mut_ptr().cast()),
                Param::end(),
            ];
            if unsafe { (api.render)(context, params.as_mut_ptr()) } < 0 {
                state.lock().unwrap().failed = true;
                stop.store(true, Ordering::Relaxed);
                break;
            }
            let mut bytes = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
            for block in &pixels {
                bytes.extend_from_slice(&block.0);
            }
            for pixel in bytes.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            let buffer = image::RgbaImage::from_raw(WIDTH, HEIGHT, bytes).unwrap();
            let frame = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
            state.lock().unwrap().frame = Some(frame);
        }
        let snapshot = state.lock().unwrap();
        let delay = if snapshot.ready && snapshot.paused {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(16)
        };
        drop(snapshot);
        std::thread::sleep(delay);
    }
    unsafe {
        (api.render_free)(context);
    }
}

#[cfg(all(test, unix))]
#[path = "../../tests/infrastructure/media.rs"]
mod tests;
