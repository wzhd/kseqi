use std::error::Error;

use std::ffi::CStr;

use std::ops::{Deref};

use std::ptr::{slice_from_raw_parts};


use x11_dl::xlib::{Xlib, _XDisplay, AnyPropertyType, XA_CARDINAL, XA_INTEGER, XA_STRING, XA_ATOM, CurrentTime};
use x11_dl::xinput2::{XInput2, XIDeviceInfo, XIAllDevices, XISlaveKeyboard, XISlavePointer, XIFloatingSlave, XI_KeyPressMask, XI_KeyReleaseMask, XIEventMask, XIGrabModifiers, XIDetachSlaveInfo, XIDetachSlave, XIKeyClass, XIAnyClassInfo, XIKeyClassInfo, XIAttachSlaveInfo, XIAttachSlave, XIMasterPointer, XIMasterKeyboard };

use crate::xdl::util::XOwnStr;

use super::{Xdll, XlibErr};
use super::err::x_expect;
use super::util::XOwnSlice;

pub fn open_xinput2(display: *mut _XDisplay,)-> Result<XInput2,  Box<dyn Error>>{
    let xip2 =XInput2::open()?;
    let mut maj = 2;
    let mut min = 2;
    if 0 != unsafe {
        (xip2.XIQueryVersion)(display, &mut maj, &mut min)
    } {
        return Err("XInput does not support version 2".into());
    }
    Ok((xip2))
}


impl Xdll {
    pub fn query_device(&self, dev: Option<i32>) -> DeviceInfos {
        let mut n = 0;
        unsafe{
            let p: *mut XIDeviceInfo = (self.xinput.XIQueryDevice)(self.display, dev.unwrap_or(XIAllDevices), &mut n);
            DeviceInfos {
                ptr: p,
                xinput: &self.xinput,
                len: n as usize,
            }
        }
    }
    pub fn dev_floating(&self, dev: i32) -> Option<bool>  {
        let binding = self.query_device(Some(dev));
        let di = binding.iter().next()?;
        Some(di.usage().ok()?. is_floating() )
    }
    pub fn dev_props(&self, dev: i32) -> XOwnSlice<u64>  {
        let mut n = 0;
        unsafe {
            let pps = (self.xinput.XIListProperties)(self.display, dev, &mut n);
            XOwnSlice::new(pps, n as usize )
        }
    }

    #[allow(dead_code)]
    pub fn get_dev_prop(&self, dev: i32, prop: u64, )-> Option<PropertyValue>  {
        let mut format = 0;
        let mut bytes_after = 0;
        let mut nitems = 0;
        let mut type_ret = 0;
        let mut data = std::ptr::null_mut();
        unsafe {
            (self.xinput.XIGetProperty)(self.display, dev,prop,
            0, 16, false as _, AnyPropertyType as u64, &mut type_ret, &mut format, &mut nitems, &mut bytes_after, &mut data);
        }
        let fla = self.intern_atom(CStr::from_bytes_with_nul(b"FLOAT\0").unwrap());
        let len = nitems as usize ;
        let x = unsafe {
            match (type_ret, format) {
                (XA_INTEGER, 8)=> PropertyValue::I8(XOwnSlice::new(data as *mut i8, len)),
                (XA_INTEGER, 16)=> PropertyValue::I16(XOwnSlice::new(data as *mut i16, len)),
                (XA_INTEGER, 32)=> PropertyValue::I32(XOwnSlice::new(data as *mut i32, len)),
                (XA_CARDINAL, 16)=> PropertyValue::U16(XOwnSlice::new(data as *mut u16, len)),
                (XA_CARDINAL, 32)=> PropertyValue::U32(XOwnSlice::new(data as *mut u32, len)),
                (XA_STRING, 8)=> PropertyValue::Str1(XOwnStr::new(data as *mut i8)),
                (XA_ATOM, _)=> {
                    let v = *(data as *mut u32);
                    PropertyValue::Atom(v)
                }
                (at, _) if at == fla && fla != 0 => PropertyValue::F32(XOwnSlice::new(data as *mut f32, len)),
                _ => {
                    error!("prop typ {} format {}", type_ret, format);
                    return None
                }
            }
        };
        Some(x)
    }

    pub fn grab_device(&self, dev: i32)-> Result<(), XlibErr> {
        let mut mb = (XI_KeyPressMask | XI_KeyReleaseMask).to_le_bytes();
        let mut evm = XIEventMask { deviceid: dev , mask_len: mb.len() as i32, mask: mb.as_mut_ptr() };
        let own = 1;
        x_expect(0, unsafe { (self.xinput.XIGrabDevice)(self.display, dev, self.rootwin, CurrentTime, 0, x11_dl::xlib::GrabModeAsync, x11_dl::xlib::GrabModeAsync, own, &mut evm) })?;
        Ok(())
    }
    pub fn ungrab_device(&self, dev: i32)-> Result<(), XlibErr> {
        x_expect(0, unsafe { (self.xinput.XIUngrabDevice)(self.display, dev,  CurrentTime) })?;
        Ok(())
    }
    pub fn grab_dev_key<const L: usize>(&self, dev: i32, kc: i32, mods: [u32; L])-> Result<(), i32> {
        let mut xmods = [XIGrabModifiers::default(); L];
        for (i, &m) in mods.iter().enumerate() {
            xmods[i].modifiers = m as i32;
        }
        let mut mb = (XI_KeyPressMask | XI_KeyReleaseMask).to_le_bytes();
        let mut evm = XIEventMask { deviceid: dev , mask_len: mb.len() as i32, mask: mb.as_mut_ptr() };
        let nfail= unsafe { (self.xinput.XIGrabKeycode)(self.display, dev, kc,  self.rootwin, x11_dl::xlib::GrabModeAsync, x11_dl::xlib::GrabModeAsync, 1, &mut evm, xmods.len() as i32, xmods.as_mut_ptr()) };
        if nfail == 0 {
            Ok(())
        }else {
            Err(nfail)
        }
    }
    #[allow(dead_code)]
    pub fn ungrab_dev_key(&self, dev: i32, kc: i32)-> Result<(), XlibErr> {
        let mut mods = [];
        x_expect(0, unsafe { (self.xinput.XIUngrabKeycode)(self.display, dev, kc,  self.rootwin,  0,mods.as_mut_ptr()) })?;
        Ok(())
    }
    pub fn detach_dev(&self, dev: i32) -> Result<(), XlibErr> {
        let mut info = XIDetachSlaveInfo::default();
        info.deviceid = dev;
        info._type = XIDetachSlave;
        let mut infos = [info];
        x_expect(0, unsafe { (self.xinput.XIChangeHierarchy)(self.display, infos.as_mut_ptr() as *mut _, 1)})?;
        Ok(())
    }
    pub fn attach_dev(&self, dev: i32, to: i32) -> Result<(), XlibErr> {
        let mut info = XIAttachSlaveInfo::default();
        info.deviceid = dev;
        info.new_master = to;
        info._type = XIAttachSlave;
        let mut infos = [info];
        x_expect(0, unsafe { (self.xinput.XIChangeHierarchy)(self.display, infos.as_mut_ptr() as *mut _, 1)})?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum PropertyValue{
    I8(XOwnSlice<i8>),
    #[allow(dead_code)]
    U8(XOwnSlice<u8>),
    I16(XOwnSlice<i16>),
    U16(XOwnSlice<u16>),
    I32(XOwnSlice<i32>),
    U32(XOwnSlice<u32>),
    F32(XOwnSlice<f32>),
    /// only the first string here
    Str1(XOwnStr),
    Atom(u32),
}

impl<'a> PropertyValue {
    pub fn get_i8(&self) -> Option<&[i8]> {
        match self{
            PropertyValue::I8(s) => Some(s.deref()),
            _ => None,
        }
    }
}

pub struct DeviceInfos<'a>{
    ptr: *mut XIDeviceInfo,
    len: usize ,
    xinput: &'a XInput2,
}

#[derive(Debug)]
pub enum Use {
    MasterPointer,
    MasterKeyboard,
    SlavePointer,
    SlaveKeyboard,
    FloatingSlave,
}

impl Use {
    pub fn is_slave(&self) -> bool {
        match self {
            Use::MasterPointer => false,
            Use::MasterKeyboard => false,
            Use::SlavePointer => true,
            Use::SlaveKeyboard => true,
            Use::FloatingSlave => true,
        }
    }
    pub fn is_floating(&self) -> bool {
        matches!(self, Use::FloatingSlave)
    }
}

impl<'a> DeviceInfos<'a> {
    fn slice(&self) -> & [XIDeviceInfo]  {
        let sl = slice_from_raw_parts(self.ptr, self.len );
        unsafe {&*sl}
    }
    pub(crate) fn iter(&self)-> impl Iterator<Item = DeviceInfo> {
        self.slice().iter().map(DeviceInfo::new)
    }
}


impl<'a> std::fmt::Debug for DeviceInfos<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.slice())
    }
}

impl<'a> Drop for DeviceInfos<'a> {
    fn drop(&mut self) {
        unsafe{
            (self.xinput.XIFreeDeviceInfo)(self.ptr)
        }
    }
}

pub struct DeviceInfo<'a> {
    xidi: &'a XIDeviceInfo
}

pub enum ClassInfo<'a> {
    Key{
        codes: &'a [i32],
    }
}

impl<'a> ClassInfo<'a> {
    fn new(c: &'a *mut XIAnyClassInfo) -> Option<Self> {
        Some(unsafe {
            let t = (**c)._type;
            match t {
                XIKeyClass => {
                    let kc = (*c) as *mut XIKeyClassInfo;
                    let kc = &*kc;
                    let kcs = slice_from_raw_parts(kc.keycodes, kc.num_keycodes as _);
                    ClassInfo::Key { codes: &*kcs }
                }
                _=> {
                    return None
                }
            }
        })
    }
    pub fn get_keyclass_info(&self) -> Option<&[i32]> {
        match self{
            ClassInfo::Key { codes } => Some(*codes)
        }
    }
}
impl<'a> DeviceInfo<'a> {
    fn new(xidi: &'a XIDeviceInfo)-> Self{
        Self{ xidi }
    }
    pub fn class_infos(&self)-> impl Iterator<Item = ClassInfo<'a>> {
        let s= slice_from_raw_parts(self.xidi.classes, self.xidi.num_classes as _);
        let _p = self.xidi as *const _;
        unsafe {
            let s: &[*mut XIAnyClassInfo] = &*s;
            s.iter().filter_map(move |c| {
                ClassInfo::new(c)
            })
        }
    }
    pub fn name(&self) -> &CStr {
        unsafe {
            CStr::from_ptr(self.xidi.name)
        }
    }
    pub fn is_enabled(&self) -> bool {
        self.xidi.enabled != 0
    }
    pub fn usage(&self) -> Result<Use, i32> {
        self.xidi._use.try_into()
    }
    pub fn id(&self) -> i32 {
        self.xidi.deviceid
    }
    pub fn attachment(&self) -> i32 {
        self.xidi.attachment
    }
}


impl<'a> std::fmt::Debug for DeviceInfo<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("DeviceInfo");
        s.field("name", &self.name())
            .field("id", &self.id())
            .field("enabled", &(self.is_enabled()));
        let attach = (self.xidi.attachment );
        let s = if self.usage().map(|u| u.is_slave()).unwrap_or(false)  {
            s.field("slave", &("master", attach))
        } else {
            s.field("master", &("pair", attach))
        };
        s .finish()
    }
}

#[allow(dead_code)]
fn get_mask(
    xl: &Xlib,
    xinput2: &XInput2,
    display: *mut _XDisplay,
    rootwin: u64,
) {
    let mut cnt = 0;
    let p = unsafe {
        (xinput2.XIGetSelectedEvents)(display, rootwin, &mut cnt)
    };
    if p.is_null() || cnt == 0 || cnt == -1{
        info!("XIGetSelectedEvents null={} masks={}",p.is_null(), cnt);
        return;
    }
    {
        let sl = slice_from_raw_parts(p, cnt as usize);
        let sl = unsafe { &*sl};
        for m in sl.iter(){
            let msl = slice_from_raw_parts(m.mask, m.mask_len as usize);
            let msl = unsafe { &*msl};
            dbg!(msl);
        }
    }
    unsafe {
        (xl.XFree)(p as _);
    }
}

impl TryFrom<i32> for Use {
    type Error=i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            XIMasterPointer => Self::MasterPointer,
            XIMasterKeyboard => Self::MasterKeyboard,
            XISlavePointer => Self::SlavePointer,
            XISlaveKeyboard => Self::SlaveKeyboard,
            XIFloatingSlave => Self::FloatingSlave,
            v => return Err(v)
        })
    }
}
