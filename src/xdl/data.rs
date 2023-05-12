use std::{marker::PhantomData, fmt::Debug, ptr::{slice_from_raw_parts}, time::Duration, borrow::Borrow};

use x11_dl::{xinput2::*, xlib};

use crate::xdl::with_xl;

use super::xinput2::Use;

#[derive(Debug)]
pub struct DeviceEvent{
    data: EventData<XIDeviceEvent>
}

pub struct HierarchyEvent{
    data: EventData<XIHierarchyEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HierarchyChange {
    MasterAdded,    
    MasterRemoved,  
    SlaveAdded,    
    SlaveRemoved,  
    SlaveAttached,  
    SlaveDetached,  
    DeviceEnabled,  
    DeviceDisabled,
}


impl Debug for HierarchyEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ds = f.debug_struct("Hierarchy");
        ds.field("time", &Duration::from_millis(self.xi_hierarchy().time))
            .field("flags", &self.flags());
        for i in self.changes() {
            ds.field("info", &i);
        }
        ds.finish()
    }
}

impl HierarchyEvent {
    pub(super)  unsafe fn new(cookie: xlib::XGenericEventCookie) -> Self {
        let data = EventData::new(cookie);
        Self { data }
    }
    pub fn infos(&self) -> &[XIHierarchyInfo] {
        let d = self.data.data();
        let bs = slice_from_raw_parts(d.info, d.num_info as usize );
        unsafe {&*bs}
    }
    /// most likely one item
    pub fn changes(&self) -> impl Iterator<Item = My<XIHierarchyInfo>>+'_  {
        self.infos().iter().filter_map(|ii| {
            if ii.flags == 0 {
                None
            }else {
                Some(My(*ii))
            }
        })
    }
    fn flags(&self) -> HierarchyFlags {
        HierarchyFlags(self.xi_hierarchy().flags)
    }
    fn xi_hierarchy(&self) -> &XIHierarchyEvent {
        self.data.data()
    }
}

trait HierarchyInfo {
    fn usage(&self) -> Result<Use, i32>;
    fn flags(&self) -> HierarchyFlags;
}

impl HierarchyInfo for XIHierarchyInfo {
    fn usage(&self) -> Result<Use, i32> {
        Use::try_from(self._use)
    }

    fn flags(&self) -> HierarchyFlags {
        HierarchyFlags(self.flags)
    }
}



impl<T: Borrow<XIHierarchyInfo>> My<T >{
    fn hierarchy_dev_info(&self)-> &XIHierarchyInfo {
        let x = self.0.borrow();
        x
    }
    pub fn deviceid(&self) -> i32  {
        self.hierarchy_dev_info().deviceid
    }
    pub fn attachment(&self) -> i32  {
        self.hierarchy_dev_info().attachment
    }
    pub fn enabled(&self) -> bool {
        self.hierarchy_dev_info().enabled == 1
    }
    pub fn flags(&self) -> HierarchyFlags  {
        HierarchyFlags(self.hierarchy_dev_info().flags)
    }

    pub fn usage(&self) -> Result<Use, i32> {
        Use::try_from(self.hierarchy_dev_info(). _use)
    }
    pub fn is_slave(&self) -> bool {
        self.usage().map(|u| u.is_slave()).unwrap_or(false )
    }
}
impl<T: Borrow<XIHierarchyInfo>> Debug for My<T >{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let h = self.0.borrow();
        let mut f = f.debug_struct("HierarchyInfo");
            f.field("dev", &h.deviceid)
            .field("attachment", &h.attachment)
            .field("enabled", &(self.enabled()))
            .field("flags", &self.flags());
        if let Ok(u) = self.usage() {
            f.field("Use", &(u));
        } else {
            f.field("Use", &"No");
        }
        f.finish()
    }
}
pub struct HierarchyFlags(i32);

impl HierarchyFlags {
    pub fn iter(&self)-> impl Iterator<Item = HierarchyChange> {
        use HierarchyChange::*;
        let i =  self.0;
        [(XIMasterAdded,  MasterAdded),
         (XIMasterRemoved,MasterRemoved),
         (XISlaveAdded,   SlaveAdded,),
         (XISlaveRemoved, SlaveRemoved,),
         (XISlaveAttached,SlaveAttached),
         (XISlaveDetached,SlaveDetached),
         (XIDeviceEnabled,DeviceEnabled),
         (XIDeviceDisabled,DeviceDisabled)
        ].iter().filter(move  |(x, _e)| x & i != 0).map(|(_, e)|*e)
    }
}


impl Debug for HierarchyFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.count_ones() == 1 {
            if let Some(h) = self.iter().next() {
                write!(f, "{:?}", h)?;
                return Ok(())
            }
        }
        f.debug_set().entries(self.iter()).finish()
    }
}

impl DeviceEvent {
    pub(super)  unsafe fn new(cookie: xlib::XGenericEventCookie) -> Self {
        let data = EventData::new(cookie);
        Self { data }
    }
    pub fn src_id(&self) -> i32 {
        self.data.data().sourceid
    }
    /// not useful for keyboard
    #[allow(dead_code)]
    fn buttons(&self) {
        let bs = self.data.data().buttons;
        let bs = slice_from_raw_parts(bs.mask, bs.mask_len as usize );
        let _bs = unsafe {&*bs};
    }
    pub fn get_key(&self)-> Option<(u8, bool)> {
        let d: &XIDeviceEvent = self.data.data();
        let p = match d.evtype {
            XI_KeyPress => {
                if (d.flags & XIKeyRepeat) != 0 {
                    return None
                }
                true
            }
            XI_KeyRelease => {
                false
            }
            _ => return None,
        };
        Some((d.detail as u8, p))
    }
}

struct EventData<T> {
    cookie: xlib::XGenericEventCookie,
    _pd: PhantomData<*mut T>,
}

impl<T: Debug> std::fmt::Debug for EventData<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventData").field("data", self.data()).finish()
    }
}

impl<T> EventData<T> {
    fn new(cookie: xlib::XGenericEventCookie) -> Self { Self { cookie, _pd: PhantomData } }

    fn data(&self)-> &T {
        unsafe {&*(self.cookie.data as *mut _)}
    }
}

impl<T> Drop for EventData<T>{
    fn drop(&mut self) {
        with_xl(|x| unsafe {
            let x = x.unwrap();
            (x.xlib.XFreeEventData)(x.display, &mut self.cookie);
        })
    }
}

pub struct My<T>(T);
