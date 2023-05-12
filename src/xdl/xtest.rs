





use x11_dl::xlib::{CurrentTime};


use super::xinput1::XinputDev;
use super::{with_xl};

pub struct Xtestdl{
    pub(crate) xt: x11_dl::xtest::Xf86vmode,
    inpdev: XinputDev,
}

impl Xtestdl {
    pub fn new(xt: x11_dl::xtest::Xf86vmode, dev: XinputDev) -> Self {
        Self { xt, inpdev: dev }
    }

    pub fn flush(&self) -> i32 {
        with_xl(|xl| {
            let xd = xl.unwrap();
            unsafe {
                (xd.xlib.XFlush)(xd.display)
            }
        })
    }
    pub fn fake_dev_key(&self, btn: u32, press: bool) -> i32 {
        let dis = with_xl(|xl| xl.unwrap().display);
        let d = self.inpdev.dev;
        let v= unsafe {
            (self.xt.XTestFakeDeviceKeyEvent)(dis, d, btn, press as i32, std::ptr::null_mut(), 0 ,CurrentTime)
        };
        self.flush();
        v
    }

    #[allow(dead_code)]
    pub fn fake_key(&self, btn: u32, press: bool) -> i32 {
        self.fake_key_delay(btn, press, CurrentTime)
    }
    pub fn fake_key_delay(&self, btn: u32, press: bool, mil:  u64) -> i32 {
        let dis = with_xl(|xl| xl.unwrap().display);
        let v= unsafe {
            (self.xt.XTestFakeKeyEvent)(dis, btn, press as i32, mil)
        };
        self.flush();
        v
    }
    pub fn fake_btn_delay(&self, btn: u32, press: bool, mil:  u64) -> i32 {
        let dis = with_xl(|xl| xl.unwrap().display);
        let v= unsafe {
            (self.xt.XTestFakeButtonEvent)(dis, btn, press as i32, mil)
        };
        self.flush();
        v
    }
    pub fn fake_btn(&self, btn: u32, press: bool) -> i32 {
        self.fake_btn_delay(btn, press, CurrentTime)
    }
}

impl Drop for XinputDev {
    fn drop(&mut self) {
        let d = self.dev;
        if !self.removed {
            unsafe{
                let _v = (self.xin.XCloseDevice)(self.disp, d);
            }
        }else {
        }
    }
}
