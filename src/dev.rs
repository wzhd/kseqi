use std::{ffi::CStr, collections::HashMap, error::Error};

use crate::xdl::xinput1::XinputDev;
use crate::xdl::{Xconn, xinput1::Xinput1, 
};

pub struct Devs {
    xinput: Xinput1,
    pub(crate) devs: HashMap<i32, DevData>,
}

impl Devs {
    pub fn new(xinput: Xinput1) -> Self { Self { xinput, devs: Default::default() } }

    pub fn add(&mut self, id: i32, attach: i32) -> Result<(), String> {
        let dd = DevData { attach , xdev: self.xinput.open_dev(id as _)?};
        let o  = self.devs.insert(id, dd).map(|_|());
        debug_assert!(o.is_none());
        Ok(())
    }
}

pub(crate) struct DevData {
    pub attach: i32,
    pub(crate) xdev: XinputDev,
}

impl std::fmt::Debug for DevData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device").field("attach", &self.attach).finish()
    }
}


pub fn find_dev(x: &Xconn) -> Result<(Option<XinputDev>, Devs), Box<dyn Error>> {
    let xin =Xinput1::new(x.display)?;
    let virt_a = x.intern_atom(CStr::from_bytes_with_nul(b"Virtual Device\0").unwrap());
    let xte_a = x.intern_atom(CStr::from_bytes_with_nul(b"XTEST Device\0").unwrap());
    let mut devs = Devs::new(xin);
    let mut xtdev = None;
    for d in x.query_device(None).iter(){
        let kcs = d.class_infos().find_map(|c| c.get_keyclass_info().map(|ks| {
            !ks.is_empty()
        })).unwrap_or(false );
        if !kcs || !d.usage().map(|n|n.is_slave()).unwrap_or(false) || !d.is_enabled() {
            continue;
        }
        let props = x.dev_props(d.id());
        if props.contains(&virt_a) && x.get_dev_prop(d.id(), virt_a).map(|p| p.get_i8() == Some(&[1])).unwrap_or(false ) {
            // when are virtual devices useful¿
            debug!("skipping virtual device {:?}",  d);
            continue;
        }
        if props.contains(&xte_a) && x.get_dev_prop(d.id(), xte_a).map(|p| p.get_i8() == Some(&[1])).unwrap_or(false ) {
            debug!("found xtest device {:?}",  d);
            let xd = devs.xinput.open_dev(d.id() as _)?;
            xtdev = Some(xd);
            continue;
        }
        debug!("found keyboard {:?}", d);
        devs.add(d.id(), d.attachment())?;
    }
    Ok( (xtdev, devs))
}
