#[macro_use]
extern crate log;

use std::collections::{BTreeSet};
use std::error::Error;
use std::process::Stdio;
use std::ffi::CStr;
use std::time::{Duration};


pub(crate) mod xdl;
mod conf;
mod dev;
mod dispatch;
mod signal;

use conf::{SmoVec};

use x11_dl::xinput2::XIHierarchyInfo;
use x11_dl::xtest;
use xdl::{Xdll, Xtestdl, DeviceEvent, with_xl, My};
pub use xdl::{Xconn, Event, keysym_to_string};

use crate::dispatch::Xdo;
use crate::xdl::HierarchyChange;

pub fn run()-> Result<(), Box<dyn Error>> {
    let x = Xconn::new()?;
    let (xtd, devs) =dev::find_dev(&x)?;
    let xf86v = match open_xf86v(&x) {
        Ok(t) => t,
        Err(e) => {
            print_install_xtest();
            return Err(e)
        }
    };
    let Some(xtd) = xtd  else {
        return Err("no xtest device".into())
    };
    let xtst = Xtestdl::new(xf86v, xtd);
    if devs.devs.is_empty() {
        return Err("no keyboard found".into())
    }
    let act_map: conf::Mapping  = conf::load_mapping(&x)?;
    x.select_change_events()?;
    for (&dev, _) in devs.devs.iter() {
        act_map. setup_device(dev, &x,  )?;
    }
    let dispat = Xdo::new(xtst);
    let mut main = Main::new(x, devs, dispat, act_map);
    main.run();
    Ok(())
}

struct Main{
    x: Xconn,
    devs: dev::Devs,
    xdo: Xdo,
    map: conf::Mapping,

    seqbuf: Vec<(u8, bool )>,
    down: BTreeSet<u8>,
    maybe: bool,
    modifiers: BTreeSet<u8>,
    floating: Option<i32,>
}

impl Main  {
    fn new(x: Xdll, devs: dev::Devs, xtst: Xdo, map: conf::Mapping) -> Self {
        let modifiers=with_xl(|xl| xl.unwrap().modifier_codes().iter().collect());
        Self {
            x, devs, map,
            xdo: xtst,
            seqbuf: vec!(), down: BTreeSet::new(),
            maybe: true,
            modifiers,
            floating: None,
        }
    }
    fn proc_xin_devent(&mut self, de: DeviceEvent) {
        let down = &mut self.down;
        if !self.devs.devs.contains_key(&de.src_id()) {
            error!("other dev {} ev {:?}", de.src_id(), de);
            return
        }
        let Some((code, press)) = de.get_key() else {return;};
        {
            let is_modifier = self.modifiers.contains(&code);
            if self.floating.is_none() {
                let fl = self.x.dev_floating(de.src_id()) == Some(true);
                if fl {
                    debug!("grab device {}", de.src_id());
                    self.floating = Some(de.src_id());
                    if let Err(e)= self.x.grab_device(de.src_id()){
                        error!("{} does not float: {e:?}", de.src_id());
                    } else {
                    }
                    if is_modifier || !press {
                        warn!("unusual grab key={code}, press={press}, ismod={is_modifier}");
                    }
                }
            }
            if !self.maybe {
                if self.floating.is_some() {
                    self.xdo.pass_key(code , press);
                }
            } else if !is_modifier && self.floating.is_none() && press   {
                debug!("not a match {} {}", code, press);
                self.maybe = false ;
            } else {
                self.seqbuf.push((code, press));
                debug!("grow seq {:?}", &self.seqbuf);
            }
            if press {
                let np = down.insert(code);
                debug_assert!(np);
            } else {
                let rm = down.remove(&code);
                if !rm {
                    debug!("unexpected key release {}", code);
                }
            }
            debug!("down keys {:?}", down);
        }
        if down.is_empty(){
            debug_assert!(!press);
            for k in self.x.query_keys_down() {
                debug!("Unpressing key {k}");
                self.xdo.pass_key(k, false);
            }
            {
                if self.maybe {
                    let sb = SmoVec::from_iter(self.seqbuf.iter().map(|(c, _p)| *c));
                    if let Some(s) = sb {
                        let seqdisp = conf::DispSeq{ sq: s.slice(), code_sym: &self.map.code_sym , sym_name: &self.map.sym_name };
                        if let Some(a) =self.map.get(&s) {
                            let acdisp = conf::DispActs { acts:  a, sym_name:  &self.map.sym_name};
                            info!("Input: {:?}, Action: {:?}", seqdisp, acdisp);
                            self.xdo.add_acts(a);
                        } else if s.slice().iter().all(|k| self.modifiers.contains(k)) {
                            info!("Input: {:?}", seqdisp);
                        } else {
                            info!("Input: {:?}, passing through", seqdisp);
                            self.xdo.add_unmatch(&self.seqbuf);
                        }
                    } else {
                        debug!("seq {:?}", &self.seqbuf);
                    }
                }
                self.seqbuf.clear();
                self.maybe = true ;
            }
            if self.floating.is_some(){
                self.unfloat();
            }
        }
    }
    fn unfloat(&mut self) {
        if let Some(d)= self.floating.take(){
            let r = self.x.ungrab_device(d);
            if let Err(e)= r  {
                error!("attach {d}  fail {e:?}");
            }
            if let Some(_t) = self.devs.devs.get(&d) {
            } else {
                error!("whereto {d}");
            }
        } else {
            warn!("no floating");
        }
    }
    fn run(&mut self) {
        loop {
            let sleep = self.xdo.proc();
            let e = self.x.recv_timeout(sleep.or_else(|| self.floating.map(|_| Duration::from_secs(1))));
            match e {
                Some(Event::Key(k)) => {
                    dbg!(&k);
                }
                Some(Event::XIDev(de)) => {
                    self.proc_xin_devent(de);
                }
                Some(Event::XIHierarchy(h)) => {
                    for hc in h.changes() {
                        self.proc_hier(hc);
                    }
                }
                Some(Event::Quit) => {
                    info!("received signal to exit");
                    break;
                }
                Some((de)) => {
                    debug!("ev {:?} ", de);
                }
                None => {
                    if self.floating.is_some() {
                        //self.unfloat();
                    }
                    continue;
                }
            }
        }
    }

    fn proc_hier(&mut self, hc: My<XIHierarchyInfo>) {
        use HierarchyChange::*;
        for c in  hc.flags().iter()  {
            match c {
                MasterAdded => (),
                MasterRemoved => (),
                SlaveAdded => {
                    if hc.enabled() {
                        info!("dev add enabled {hc:?}");
                        self.add_dev(&hc);
                    } else {
                        debug!("dev added without enabling {hc:?}");
                    }
                }
                SlaveRemoved => {
                    if self.devs.devs.remove(&hc.deviceid()).is_some(){
                        info!("device removed {hc:?}");
                    }
                }
                SlaveAttached => {
                    info!("when is it attached instead of added? {hc:?}");
                    if hc.enabled() {
                        self.add_dev(&hc);
                    }
                }
                SlaveDetached => {
                    info!("when is it Detached instead of removed? {hc:?}");
                    self.devs.devs.remove(&hc.deviceid());
                }
                DeviceEnabled => {
                    if hc.is_slave() {
                        self.add_dev(&hc) ;
                    } else {
                        info!("ignoring dev enabling {hc:?}");
                    }
                }
                DeviceDisabled => {
                    if let Some(mut d) =  self.devs.devs.remove(&hc.deviceid()){
                        d.xdev.set_removed();
                        info!("device disabled {hc:?}");
                    }
                }
            }
        }
    }
    fn add_dev(&mut self,  hc: &My<XIHierarchyInfo>) {
        let id = hc.deviceid();
        let binding = self.x.query_device(Some(id));
        let Some(di) = binding.iter().next() else {
            error!("dev {id} not found") ;
            return
        };
        self.xdo.ondevchange();
        if !di.class_infos().find_map(|c| c.get_keyclass_info().map(|ks| {
            !ks.is_empty()
        })).unwrap_or(false ){
            debug!("dev {id} is not keyed");
            return
        }
        info!("Enabling device {}", di.name().to_str().unwrap_or_default());
        if self.devs.devs.contains_key(&hc.deviceid()) {
            error!("dev {id} already added") ;
            return
        }
        if let Err(ar) = self.devs.add(hc.deviceid(),  hc.attachment()) {
            warn!("add dev result {ar:?}");
            return;
        }
        if let Err(e) =  self.map.setup_device(id, &self.x,) {
            error!("setting up device {id} fail: {e:?}")
        }
    }
}




fn print_install_xtest() {
    let variants = [
        ("pacman", "-Sy", "libxtst"),
        ("apt-get", "install", "libxtst6"),
        ("zypper", "install", "libXtst6"),
        ("dnf", "-y install", "libXtst"),
        ("yum", "install", "libXtst"),
        ("emerge", "", "x11-libs/libXtst"),
    ];
    for (e, a, p) in variants {
        if command_exist(e) {
            error!("To get XTEST, try {} {} {} ", e, a, p);
            break;
        }
    }
}

fn open_xf86v(x: &Xdll)-> Result<xtest::Xf86vmode, Box<dyn Error>> {
    let ext = x.query_extension( CStr::from_bytes_with_nul(b"XTEST\0").unwrap());
    if ext.is_none(){
        return Err("XTEST unavailable".to_string().into());
    }
    let v= xtest::Xf86vmode::open()?;
    xdl::with_xl(|xl|  xl.map(|_|()) .map_err(move |e| e.clone()))?;
    Ok(v)
}

fn command_exist(exe: &str) -> bool {
    std::process::Command::new(exe)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn().is_ok()
}

impl Drop for Main{
    fn drop(&mut self) {
        if let Some(d) = self.floating {
            info!("ungrab device {d}");
            self.unfloat();
            let _i = with_xl(|x| x.unwrap().sync());
        }
    }
}
