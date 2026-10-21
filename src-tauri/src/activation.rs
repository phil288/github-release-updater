//! Hands the launcher's activation token of a second instance to the running one.
//!
//! The desktop (GNOME on Wayland in particular) gives every launch an activation
//! token and shows a busy cursor until a window is activated with it. A second
//! instance exits as soon as single-instance forwards its arguments, so its token
//! would never be used and the cursor would keep spinning. The second instance
//! saves the token to a file; the running instance reads it and passes it to GDK
//! before raising the window. The single-instance plugin forwards only argv, which
//! cannot carry the token, hence the file.

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::{c_char, c_void, CString};
    use std::path::PathBuf;

    #[link(name = "gdk-3")]
    extern "C" {
        fn gdk_display_get_default() -> *mut c_void;
        fn gdk_wayland_display_get_type() -> usize;
        fn gdk_wayland_display_set_startup_notification_id(display: *mut c_void, id: *const c_char);
        fn gdk_notify_startup_complete_with_id(id: *const c_char);
    }
    #[link(name = "gobject-2.0")]
    extern "C" {
        fn g_type_check_instance_is_a(instance: *mut c_void, iface_type: usize) -> i32;
    }

    fn token_file() -> Option<PathBuf> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
        Some(PathBuf::from(dir).join("dev.phil.gh-release-updater.activation-token"))
    }

    /// Saves this launch's token, or removes a stale one when there is none.
    pub fn save_launch_token() {
        let Some(path) = token_file() else { return };
        let token = std::env::var("XDG_ACTIVATION_TOKEN")
            .or_else(|_| std::env::var("DESKTOP_STARTUP_ID"))
            .ok()
            .filter(|t| !t.is_empty());
        let _ = match token {
            Some(t) => std::fs::write(&path, t),
            None => std::fs::remove_file(&path),
        };
    }

    /// Takes the token saved by the latest launch. Must run on the main thread,
    /// before the window is focused: GDK activates with it on the next focus.
    pub fn use_forwarded_token() {
        let Some(path) = token_file() else { return };
        let Ok(token) = std::fs::read_to_string(&path) else { return };
        let _ = std::fs::remove_file(&path);
        let Ok(token) = CString::new(token.trim()) else { return };
        if token.is_empty() {
            return;
        }
        // SAFETY: called on the GTK main thread after GTK is initialised; the
        // Wayland setter is only called on a GdkWaylandDisplay.
        unsafe {
            let display = gdk_display_get_default();
            if display.is_null() {
                return;
            }
            if g_type_check_instance_is_a(display, gdk_wayland_display_get_type()) != 0 {
                gdk_wayland_display_set_startup_notification_id(display, token.as_ptr());
            } else {
                gdk_notify_startup_complete_with_id(token.as_ptr());
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub fn save_launch_token() {}
    pub fn use_forwarded_token() {}
}

pub use imp::{save_launch_token, use_forwarded_token};
