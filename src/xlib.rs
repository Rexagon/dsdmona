use std::ffi::CString;
use std::time::Duration;

use anyhow::{anyhow, Result};
use x11::xlib;

pub struct XDisplay {
    display: *mut xlib::Display,
}

impl XDisplay {
    pub fn open(name: String) -> Result<Self> {
        let display_name = CString::new(name)?;

        for _ in 0..50 {
            let display = unsafe { xlib::XOpenDisplay(display_name.as_ptr()) };
            if !display.is_null() {
                return Ok(Self { display });
            } else {
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        Err(anyhow!("Failed to open X Display"))
    }
}

impl Drop for XDisplay {
    fn drop(&mut self) {
        unsafe { xlib::XCloseDisplay(self.display) };
    }
}
