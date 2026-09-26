//! brixo:// links on macOS. Clicking Play opens (or wakes) Brixo Player and
//! macOS then sends the link as an Apple Event, which mac_links.m catches
//! and queues here. The player checks the queue every frame, so Play works
//! whether or not it was already open. Elsewhere links arrive as the
//! command-line argument instead, and this does nothing.

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, CStr};
    use std::sync::Mutex;

    static LINKS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    unsafe extern "C" {
        fn brixo_listen_for_links(on_link: extern "C" fn(*const c_char));
    }

    extern "C" fn on_link(url: *const c_char) {
        if url.is_null() {
            return;
        }
        let link = unsafe { CStr::from_ptr(url) }.to_string_lossy().into_owned();
        LINKS.lock().unwrap().push(link);
    }

    /// Call once, before the window opens.
    pub fn listen() {
        unsafe { brixo_listen_for_links(on_link) }
    }

    /// The oldest link not yet handled.
    pub fn take() -> Option<String> {
        let mut links = LINKS.lock().unwrap();
        (!links.is_empty()).then(|| links.remove(0))
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn listen() {}
    pub fn take() -> Option<String> {
        None
    }
}

pub use imp::{listen, take};
