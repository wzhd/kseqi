use std::rc::Rc;

use x11_dl::error::OpenError;
use x11_dl::xinput::{XInput, XDevice};
use x11_dl::xlib::_XDisplay;

pub struct Xinput1{
    xin: Rc< XInput>,
    disp: *mut _XDisplay,
}

impl Xinput1 {
    pub fn new(disp: *mut _XDisplay) -> Result<Xinput1, OpenError> {
        let xin: XInput = XInput::open()?;
        let xin= Rc::new(xin);
        Ok(Xinput1{ xin, disp })
    }
    pub fn open_dev(&self, d: u64) -> Result<XinputDev, String> {
        let xd: *mut XDevice = unsafe{ (self.xin.XOpenDevice)(self.disp, d)};
        if xd.is_null(){
            return Err(format!("Device {d} is null"))
        }
         Ok(XinputDev{ xin: self.xin.clone(), disp: self.disp, dev: xd, removed: false })
    }
    #[allow(dead_code)]
    pub fn query_dev_state(&self, dev: *mut XDevice) {
        unsafe{
            let _s: *mut x11_dl::xinput::XDeviceState =(self.xin.XQueryDeviceState)(self.disp, dev);
        }
    }
}

pub struct XinputDev{
    pub(crate) xin: std::rc::Rc<XInput>,
    pub(crate) disp: *mut _XDisplay,
    pub(crate) dev: *mut XDevice,
    pub(super) removed: bool,
}

impl XinputDev {
    pub fn set_removed(&mut self) {
        self.removed = true;
    }
}
