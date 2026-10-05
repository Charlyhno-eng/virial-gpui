//! Copy-only XDND sources. The existing XInput implicit grab supplies motion
//! and release events even while the pointer is outside the originating window.
use super::client::{X11Client, X11ClientState, X11ClientStatePtr};
use crate::{FileDropEvent, PlatformInput};
use calloop::timer::{TimeoutAction, Timer};
use std::time::Duration;
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{self, AtomEnum, ConnectionExt as _, EventMask, PropMode},
    },
    wrapper::ConnectionExt as _,
};

pub(super) struct OutgoingDrag {
    pub(super) source: u32,
    target: u32,
    proxy: u32,
    version: u32,
    uris: Vec<u8>,
    accepted: bool,
    awaiting_status: bool,
    pending_position: Option<(u32, u32)>,
    released: Option<u32>,
}

fn message(state: &X11ClientState, target: u32, proxy: u32, kind: u32, data: [u32; 5]) {
    let event = xproto::ClientMessageEvent::new(32, target, kind, data);
    let _ = state
        .xcb_connection
        .send_event(false, proxy, EventMask::NO_EVENT, event);
    let _ = state.xcb_connection.flush();
}

fn property(state: &X11ClientState, window: u32, atom: u32, kind: AtomEnum) -> Option<u32> {
    if window == 0 {
        return None;
    }
    state
        .xcb_connection
        .get_property(false, window, atom, kind, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}

impl X11ClientStatePtr {
    pub fn start_file_drag(&self, source: u32, uris: String) -> bool {
        let Some(client) = self.0.upgrade() else {
            return false;
        };
        let mut state = client.borrow_mut();
        if state.outgoing_drag.is_some() {
            return false;
        }
        if state
            .xcb_connection
            .set_selection_owner(source, state.atoms.XdndSelection, x11rb::CURRENT_TIME)
            .ok()
            .and_then(|cookie| cookie.check().ok())
            .is_none()
        {
            return false;
        }
        state.outgoing_drag = Some(OutgoingDrag {
            source,
            target: 0,
            proxy: 0,
            version: 5,
            uris: uris.into_bytes(),
            accepted: false,
            awaiting_status: false,
            pending_position: None,
            released: None,
        });
        let _ = state.xcb_connection.flush();
        true
    }

    pub fn cancel_file_drag(&self) {
        if let Some(client) = self.0.upgrade() {
            client.borrow_mut().cancel_outgoing_drag();
        }
    }
}

impl X11ClientState {
    pub(super) fn cancel_outgoing_drag(&mut self) {
        if let Some(drag) = self.outgoing_drag.take() {
            if drag.target != 0 && drag.released.is_none() {
                message(
                    self,
                    drag.target,
                    drag.proxy,
                    self.atoms.XdndLeave,
                    [drag.source, 0, 0, 0, 0],
                );
            }
            // Do not clear a selection already claimed by another application.
            if self
                .xcb_connection
                .get_selection_owner(self.atoms.XdndSelection)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .is_some_and(|reply| reply.owner == drag.source)
            {
                let _ = self.xcb_connection.set_selection_owner(
                    x11rb::NONE,
                    self.atoms.XdndSelection,
                    x11rb::CURRENT_TIME,
                );
            }
            let _ = self.xcb_connection.flush();
        }
    }

    pub(super) fn move_outgoing_drag(&mut self, time: u32) {
        let Some(drag) = &self.outgoing_drag else {
            return;
        };
        if drag.released.is_some() {
            return;
        }
        let source = drag.source;
        let root = self.xcb_connection.setup().roots[self.x_root_index].root;
        let Some(pointer) = self
            .xcb_connection
            .query_pointer(root)
            .ok()
            .and_then(|c| c.reply().ok())
        else {
            return;
        };
        let packed = ((pointer.root_x as u16 as u32) << 16) | pointer.root_y as u16 as u32;
        let mut window = pointer.child;
        let mut target = 0;
        let mut version = 5;
        // Descend from the frame to the application and any embedded target.
        for _ in 0..64 {
            if window == 0 {
                break;
            }
            if window == source {
                target = 0;
                break;
            }
            if let Some(v) = property(self, window, self.atoms.XdndAware, AtomEnum::ATOM) {
                if v >= 3 {
                    target = window;
                    version = v.min(5);
                }
            }
            let Some(reply) = self
                .xcb_connection
                .query_pointer(window)
                .ok()
                .and_then(|c| c.reply().ok())
            else {
                break;
            };
            window = reply.child;
        }
        let proxy = property(self, target, self.atoms.XdndProxy, AtomEnum::WINDOW)
            .filter(|p| property(self, *p, self.atoms.XdndProxy, AtomEnum::WINDOW) == Some(*p))
            .unwrap_or(target);
        let mut drag = self.outgoing_drag.take().unwrap();
        if target != drag.target {
            if drag.target != 0 {
                message(
                    self,
                    drag.target,
                    drag.proxy,
                    self.atoms.XdndLeave,
                    [source, 0, 0, 0, 0],
                );
            }
            drag.target = target;
            drag.proxy = proxy;
            drag.version = version;
            drag.accepted = false;
            drag.awaiting_status = false;
            drag.pending_position = None;
            if target != 0 {
                message(
                    self,
                    target,
                    proxy,
                    self.atoms.XdndEnter,
                    [source, version << 24, self.atoms.TextUriList, 0, 0],
                );
            }
        }
        if target != 0 {
            if drag.awaiting_status {
                drag.pending_position = Some((packed, time));
            } else {
                message(
                    self,
                    target,
                    proxy,
                    self.atoms.XdndPosition,
                    [source, 0, packed, time, self.atoms.XdndActionCopy],
                );
                drag.awaiting_status = true;
            }
        }
        self.outgoing_drag = Some(drag);
    }

    fn finish_outgoing_release(&mut self) {
        let Some(drag) = &self.outgoing_drag else {
            return;
        };
        let Some(time) = drag.released else {
            return;
        };
        if drag.awaiting_status {
            return;
        }
        if drag.target != 0 && drag.accepted {
            message(
                self,
                drag.target,
                drag.proxy,
                self.atoms.XdndDrop,
                [drag.source, 0, time, 0, 0],
            );
        } else {
            // No drop was sent: notify the target that this drag is over.
            let drag = self.outgoing_drag.as_mut().unwrap();
            drag.released = None;
            self.cancel_outgoing_drag();
        }
    }
}

impl X11Client {
    /// Return true when a native outgoing operation handled this event.
    pub(super) fn outgoing_drag_event(&self, event: &Event) -> bool {
        let mut state = self.0.borrow_mut();
        if state.outgoing_drag.is_none() {
            return false;
        }
        match event {
            Event::SelectionRequest(event) if event.selection == state.atoms.XdndSelection => {
                let drag = state.outgoing_drag.as_ref().unwrap();
                let property = if event.property == 0 {
                    event.target
                } else {
                    event.property
                };
                let success = if event.target == state.atoms.TextUriList {
                    state
                        .xcb_connection
                        .change_property8(
                            PropMode::REPLACE,
                            event.requestor,
                            property,
                            event.target,
                            &drag.uris,
                        )
                        .ok()
                        .and_then(|c| c.check().ok())
                        .is_some()
                } else if event.target == state.atoms.TARGETS {
                    state
                        .xcb_connection
                        .change_property32(
                            PropMode::REPLACE,
                            event.requestor,
                            property,
                            AtomEnum::ATOM,
                            &[state.atoms.TARGETS, state.atoms.TextUriList],
                        )
                        .ok()
                        .and_then(|c| c.check().ok())
                        .is_some()
                } else {
                    false
                };
                let reply = xproto::SelectionNotifyEvent {
                    response_type: xproto::SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: event.time,
                    requestor: event.requestor,
                    selection: event.selection,
                    target: event.target,
                    property: if success { property } else { 0 },
                };
                let _ = state.xcb_connection.send_event(
                    false,
                    event.requestor,
                    EventMask::NO_EVENT,
                    reply,
                );
                let _ = state.xcb_connection.flush();
                true
            }
            Event::SelectionClear(event) if event.selection == state.atoms.XdndSelection => {
                let source = state.outgoing_drag.as_ref().unwrap().source;
                state.cancel_outgoing_drag();
                drop(state);
                if let Some(window) = self.get_window(source) {
                    window.handle_input(PlatformInput::FileDrop(FileDropEvent::NativeDragFinished));
                }
                true
            }
            Event::ClientMessage(event) if event.type_ == state.atoms.XdndStatus => {
                let [target, flags, _, _, action] = event.data.as_data32();
                let copy = state.atoms.XdndActionCopy;
                let drag = state.outgoing_drag.as_mut().unwrap();
                if event.window != drag.source || target != drag.target {
                    return true;
                }
                drag.accepted = flags & 1 != 0 && (drag.version < 5 || action == copy);
                drag.awaiting_status = false;
                let pending = drag.pending_position.take();
                if let Some((packed, time)) = pending {
                    let (source, target, proxy) = (drag.source, drag.target, drag.proxy);
                    drag.awaiting_status = true;
                    message(
                        &state,
                        target,
                        proxy,
                        state.atoms.XdndPosition,
                        [source, 0, packed, time, copy],
                    );
                } else {
                    state.finish_outgoing_release();
                }
                true
            }
            Event::ClientMessage(event) if event.type_ == state.atoms.XdndFinished => {
                let drag = state.outgoing_drag.as_ref().unwrap();
                if event.window == drag.source
                    && event.data.as_data32()[0] == drag.target
                    && drag.released.is_some()
                {
                    state.cancel_outgoing_drag();
                }
                true
            }
            Event::XinputMotion(event) => {
                state.move_outgoing_drag(event.time);
                false
            }
            Event::XinputButtonRelease(event) if event.detail == 1 => {
                // A previous receiver can still be reading while a new internal
                // drag runs. Its eventual completion must not consume this release.
                if state.outgoing_drag.as_ref().unwrap().released.is_some() {
                    return false;
                }
                state.move_outgoing_drag(event.time);
                let drag = state.outgoing_drag.as_ref().unwrap();
                let source = drag.source;
                if drag.target == 0 {
                    state.cancel_outgoing_drag();
                    return false; // Normal internal GPUI drop or release outside any target.
                }
                state.outgoing_drag.as_mut().unwrap().released = Some(event.time);
                state.finish_outgoing_release();
                // Keep serving URI requests until Finished; a disappearing target must
                // not leave a selection source alive forever.
                let release_time = event.time;
                let _ =
                    state.loop_handle.insert_source(
                        Timer::from_duration(Duration::from_secs(30)),
                        move |_, _, client| {
                            let mut state = client.0.borrow_mut();
                            if state.outgoing_drag.as_ref().is_some_and(|d| {
                                d.source == source && d.released == Some(release_time)
                            }) {
                                state.cancel_outgoing_drag();
                            }
                            TimeoutAction::Drop
                        },
                    );
                drop(state);
                if let Some(window) = self.get_window(source) {
                    window.handle_input(PlatformInput::FileDrop(FileDropEvent::NativeDragFinished));
                }
                true
            }
            _ => false,
        }
    }
}
