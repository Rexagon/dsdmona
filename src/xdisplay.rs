use std::ffi::CString;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use x11::xlib;

pub struct XDisplay {
    display: *mut xlib::Display,
}

impl XDisplay {
    pub fn find_free_xdisplay() -> Option<u8> {
        for i in 0..32 {
            let lock = format!("/tmp/.X{}-lock", i);
            if !Path::new(&lock).exists() {
                return Some(i);
            }
        }
        None
    }

    pub fn open(display_name: &str) -> Result<Self> {
        let display_name = CString::new(display_name)?;

        for _ in 0..50 {
            let display = unsafe { xlib::XOpenDisplay(display_name.as_ptr()) };
            if !display.is_null() {
                return Ok(Self { display });
            } else {
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        anyhow::bail!("Failed to open X Display")
    }
}

impl Drop for XDisplay {
    fn drop(&mut self) {
        unsafe { xlib::XCloseDisplay(self.display) };
    }
}
