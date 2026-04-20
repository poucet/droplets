//! Native drag-out of exported MIDI files.
//!
//! The plugin runs inside a DAW window we don't own. When the user presses
//! and holds on a drag handle in the webview UI, the frontend POSTs an IPC
//! message; Rust then needs to (a) materialize a `.mid` file, (b) start an
//! OS-native drag from the parent DAW window so it can be dropped onto any
//! arranger track.
//!
//! The tricky bit is the window handle. `drag::start_drag` on macOS/Windows
//! takes a `&impl HasWindowHandle`. The DAW passes us its window via the
//! CLAP GUI extension's `set_parent` hook; we stash the resulting raw
//! handle here so the IPC handler can build a transient `WindowHandle` out
//! of it on demand.
//!
//! Linux is not supported in v1 — `drag` on Linux requires a
//! `gtk::ApplicationWindow`, which we don't have inside a CLAP plugin.
//! Callers on Linux fall back to writing the file and revealing it in the
//! file manager so the user can drag from there.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};

/// Wrapper around a raw parent-window handle that implements
/// `HasWindowHandle`. Unsafe because we're asserting the underlying window
/// outlives any borrow we hand out — in practice the DAW owns the window
/// and keeps it alive for as long as the plugin is loaded, so drag calls
/// during the plugin's lifetime are safe.
///
/// The explicit Send+Sync impls are safe because:
/// - On macOS the inner pointer is an `NSView*` that the AppKit main
///   thread owns; we call drag from the main thread via the IPC handler
///   on that thread.
/// - On Windows the inner value is an `HWND`, which is documented to be
///   thread-safe as an opaque handle.
/// - On X11 it's an `xcb_window_t` (u32) — also safe to move.
pub struct DragWindow(RawWindowHandle);

unsafe impl Send for DragWindow {}
unsafe impl Sync for DragWindow {}

impl DragWindow {
    pub fn new(raw: RawWindowHandle) -> Self {
        Self(raw)
    }
}

impl HasWindowHandle for DragWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: We rely on the DAW keeping its plugin window alive for
        // the lifetime of the plugin. If the user somehow unloaded the
        // plugin mid-drag (which the DAW prevents anyway — the drag
        // gesture blocks the DAW's event loop), the window pointer would
        // already be invalid before we got here.
        unsafe { Ok(WindowHandle::borrow_raw(self.0)) }
    }
}

/// Shared slot the plugin populates on `set_parent` and the IPC handler
/// reads when a drag is requested. `None` means the GUI hasn't been
/// parented yet (no drag possible); `Some` means we have a handle to drag
/// from.
pub type DragState = Arc<Mutex<Option<DragWindow>>>;

/// Result of an attempted drag. Separate from the DAW's perspective of
/// drop success — we don't wait for that.
#[derive(Debug)]
pub enum DragStart {
    /// Native drag started; OS handles the rest.
    Started,
    /// Drag not supported on this platform — caller should fall back to
    /// revealing the file in the user's file manager (15b.5).
    Unsupported,
    /// Tried to start a drag but the window handle wasn't available
    /// (plugin not yet parented, or parent closed). Caller logs + ignores.
    NoWindow,
    /// The drag crate returned an error. Surfaces the stringified error.
    Failed(String),
}

/// Kick off a native OS drag carrying a single file. Runs synchronously
/// on whatever thread called it — on macOS and Windows the caller must
/// be on the main/UI thread (the same thread the DAW uses to pump events),
/// or the drag may appear to start but not track the mouse correctly.
///
/// On Linux this is a no-op returning `Unsupported` until we have a GTK
/// window to drag from.
pub fn start_file_drag(state: &DragState, file: PathBuf) -> DragStart {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let guard = state.lock().unwrap();
        let Some(window) = guard.as_ref() else {
            return DragStart::NoWindow;
        };

        let item = drag::DragItem::Files(vec![file]);
        // No preview image — most DAWs show their own ghost of the
        // dragged file, and building a placeholder PNG adds a surprising
        // amount of code (platform-specific icon encoding).
        let preview = drag::Image::Raw(Vec::new());

        match drag::start_drag(
            window,
            item,
            preview,
            |_result, _cursor| {
                // Drop callback: nothing to do. Temp file cleanup runs
                // on a timer regardless of drag outcome.
            },
            drag::Options::default(),
        ) {
            Ok(_) => DragStart::Started,
            Err(e) => DragStart::Failed(format!("{}", e)),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (state, file);
        DragStart::Unsupported
    }
}

/// Fallback for platforms where drag isn't supported (Linux) or when the
/// drag attempt fails: reveal the file in the user's file manager so they
/// can drag it from there. Best-effort; logs on failure.
pub fn reveal_fallback(file: &Path) {
    if let Err(e) = open::that(file) {
        log::warn!(
            "drag fallback: failed to reveal {} in file manager: {}",
            file.display(),
            e
        );
    }
}
