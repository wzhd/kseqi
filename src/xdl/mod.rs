
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use std::error::Error;
use std::ffi::CStr;

use std::mem::MaybeUninit;
use std::num::{NonZeroU8, NonZeroU64};
use std::ops::Index;


use std::rc::Rc;
use std::time::{Duration, Instant};

use mio::Events;
use mio::{unix::SourceFd, Poll, Token, Interest};


use x11_dl::error::OpenError;
use x11_dl::xinput2::{XI_KeyPressMask, XI_KeyReleaseMask, XIEventMask, XIAllDevices, XInput2, XI_DeviceChanged, XI_DeviceChangedMask, XI_HierarchyChangedMask, XIDeviceChangedEvent, XI_KeyPress, XI_KeyRelease, XI_HierarchyChanged};
use x11_dl::xlib::{Xlib, _XDisplay, XEvent, NoSymbol, self, XKeyEvent, KeyPressMask, CurrentTime, KeyPress, KeyRelease, KeySym, KeyCode, XMappingEvent};

mod xtest;
mod err;
pub(crate) mod xinput1;
pub mod xinput2;
pub(crate) mod keysym;
mod util;
mod data;

pub use xtest::Xtestdl;
pub use err::{XlibErr, x_err};

use crate::signal::{quit_recver, LAST_SIG};
use crate::xdl::util::XOwnSlice;
pub use self::data::DeviceEvent;
pub use self::data::{HierarchyEvent, HierarchyChange, My};
use self::err::{x_err_o, x_expect};
use self::util::XOwnStr;
use self::xinput2::open_xinput2;

const X_TOKEN: Token = Token(0);
const SIG_TOKEN: Token = Token(1);
const XKB_USE_CORE_KBD: u32 = 0x0100;

thread_local! {
    static LOCAL_X: RefCell<LazyValue<Rc<XlibDpy>, OpenError >> = RefCell::default();
}

#[derive(Default)]
enum LazyValue<T, E>{
    #[default]
    Uninit,
    Ok(T),
    Err(E),
}

impl<T, E> LazyValue<T, E> {
    fn uninit(&self) -> bool {
        matches!(self, LazyValue::Uninit)
    }
}

pub(crate) fn with_xl<R, F>(call: F)-> R
where F: FnOnce(Result<&Rc<XlibDpy>, &OpenError>) -> R {
    LOCAL_X.with(|lz|{
        if lz.borrow().uninit() {
            match Xlib::open() {
                Ok(xlib) => {
                    let display: *mut _XDisplay = unsafe{ (xlib.XOpenDisplay)(&0)};
                    let xd = XlibDpy{ xlib, display };
                    *lz.borrow_mut() = LazyValue::Ok(Rc::new(xd));
                }
                Err(e) => {
                    *lz.borrow_mut() = LazyValue::Err(e);
                }
            }
        }
        match &*lz.borrow() {
            LazyValue::Uninit => panic!("should init"),
            LazyValue::Ok(v) => call(Ok(v)),
            LazyValue::Err(e) => call(Err(e)),
        }
    })
}

pub(crate) fn get_x()-> Result<Rc<XlibDpy>, OpenError > {
    with_xl(|r| {
        r.cloned().map_err(|e| e.clone())
    })
}
pub type Xdll = Xconn;

pub(crate) struct XlibDpy {
    xlib: Xlib,
    display: *mut _XDisplay,
}

pub struct Xconn {
    xdp: Rc<XlibDpy>,
    rootwin: u64,
    pub(crate) display: *mut _XDisplay,
    poll: Poll,
    poll_events: Events,
    xinput: XInput2,
    xiopcode: i32,
}

extern "C" fn x_error_callback(
    disp: *mut xlib::Display,
    event: *mut xlib::XErrorEvent,
) -> i32 {
    unsafe {
        let event = (&*event);
        let ec = event.error_code;
        let mut buf = [0; 512];
        let c = with_xl(|r| {
            let xd = r.unwrap();
            (xd.xlib.XGetErrorText)(disp, ec as _, buf.as_mut_ptr(), buf.len() as _)
        });
        if c != 0 {
            error!("fail XGetErrorText error {:?}", c);
            return 0
        }
        let s = CStr::from_ptr(buf.as_ptr());
        error!(
            "x error {:?}. code={ec} request={} minor={} resource={} typ={}",
            s, event.request_code, event.minor_code, event.resourceid, event.type_);
    }
    0
}
impl Xdll {
    pub fn new() -> Result<Xdll, Box<dyn Error>> {
        Self::newx(get_x()?)
    }
    pub(crate) fn newx(xdpy: Rc<XlibDpy>) -> Result<Self,  Box<dyn Error>> {
        let xlib = &xdpy.xlib;
        unsafe{ (xlib.XSetErrorHandler)(Some(x_error_callback))};
        let (display, rootwin, fd) = unsafe {
            let display: *mut _XDisplay = (xlib.XOpenDisplay)(&0);
            let rootwin: u64 =  (xlib.XDefaultRootWindow)(display) ;
            let fd = (xlib.XConnectionNumber)(display);
            (display, rootwin, fd)
        };
        let xies = CStr::from_bytes_with_nul(b"XInputExtension\0").unwrap();
        let Some((xiopcode, _event, _error)) =  query_extension(xlib, display, xies)  else {
            let err = "XInput extension is not supported!";
            return Err(err.into());
        };
        let xinput: XInput2 = open_xinput2(display)?;
        // if it fails, probably reached limit or lack memory
        let poll = Poll::new().expect("can't create epoll");
        poll.registry().register(&mut SourceFd(&fd), X_TOKEN,  Interest::READABLE ).expect("register poll x fd");

        let mut sigr = quit_recver()?;
        poll.registry().register(&mut sigr, SIG_TOKEN,  Interest::READABLE ).expect("register poll sig");
        Box::leak(Box::new(sigr));
        let events = Events::with_capacity(8);
        Ok(Self {
            xdp: xdpy,
            rootwin,
            display,
            poll,
            poll_events: events,
            xinput,
            xiopcode,
        })
    }
    fn xlib(&self) -> &Xlib  {
        &self.xdp.xlib
    }
    #[allow(dead_code)]
    pub fn recv_xevent_blocking(&self)-> Result<XEvent, XlibErr> {
        let mut xevent = MaybeUninit::uninit();
        unsafe {
            let r = (self.xlib().XNextEvent)(self.display, xevent.as_mut_ptr());
            x_err_o(r, 0)?;
            debug!("xne");
            Ok(xevent.assume_init())
        }
    }
    pub fn recv_timeout(&mut self, dur: Option<Duration>)-> Option<Event> {
        let beg = Instant::now();
        let ddl = dur.map(|d| beg+d);
        loop {
            while let Some(e) = self.pop_event() {
                match &e {
                    Event::Key(ke) if !ke.is_press() => {
                        let ke = ke.x;
                        if let Some(re) = self.pop_event_repress(ke.keycode, ke.time){
                            debug_assert_eq!(re.keycode, ke.keycode);
                            info!("skip rep of {} in {:?}", ke.keycode, ke.time);
                            continue;
                        }
                    }
                    _ => (),
                }
                return Some(e)
            }
            let remt = ddl.map(|t| t.saturating_duration_since( Instant::now()));
            if remt.map(|t| t.is_zero()) .unwrap_or(false ) {
                return None
            }
            if let Err(e) = self.poll.poll(&mut self.poll_events, remt) {
                if e.raw_os_error() != Some(libc::EINTR) {
                    error!("Poll err {e:?}");
                }
            }
            for e in self.poll_events.iter(){
                if SIG_TOKEN == e.token() {
                    debug!("got sig {}", LAST_SIG.load(std::sync::atomic::Ordering::Relaxed));
                    if !e.is_read_closed() {
                        error!("expecting end");
                    }
                    return Some(Event::Quit)
                }
            }
            self.poll_events.clear();
        }
    }
    pub fn select_change_events(&self)-> Result<(),  Box<dyn Error>> {
        let mut mb: [u8; 4] = (XI_DeviceChangedMask
                      | XI_HierarchyChangedMask
        ).to_le_bytes();
        let mut evm = XIEventMask { deviceid: XIAllDevices , mask_len: mb.len() as i32, mask: mb.as_mut_ptr() };
        unsafe {
            x_expect(0, (self.xinput.XISelectEvents)(self.display, self.rootwin, &mut evm, 1))?;
        }
        Ok(())
    }
    pub fn select_dev_events(&self, dev: i32)-> Result<(),  XlibErr> {
        let mut mb = (XI_KeyPressMask | XI_KeyReleaseMask
        ).to_le_bytes();
        let mut evm = XIEventMask { deviceid: dev , mask_len: mb.len() as i32, mask: mb.as_mut_ptr() };
        unsafe {
            x_expect(0, (self.xinput.XISelectEvents)(self.display, self.rootwin, &mut evm, 1))?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn grab_keysym(&self, sym: u64 ,  mask: u32 )-> Result<(), Option<XlibErr>> {
        let c = self.keysym_to_keycode(sym).ok_or(None)?;
        self.grab_key(c.get() as i32, mask).map_err(|e| e.into())
    }
    pub fn grab_key(&self, kc: i32,  mask: u32)-> Result<(), XlibErr> {
        let v = unsafe { (self.xlib().XGrabKey)(self.display, kc, mask, self.rootwin, false as _, x11_dl::xlib::GrabModeSync, x11_dl::xlib::GrabModeAsync) };
        x_err(v)?;
        Ok(())
    }
    #[allow(dead_code)]
    pub fn grab_keyboard(&self)-> Result<(), XlibErr> {
        x_expect(0, unsafe { (self.xlib().XGrabKeyboard)(self.display, self.rootwin, false as _, x11_dl::xlib::GrabModeSync, x11_dl::xlib::GrabModeAsync, CurrentTime) })?;
        Ok(())
    }
    pub fn keysym_to_keycode(&self, sym: u64) -> Option<NonZeroU8> {
        let kc = unsafe {  (self.xlib().XKeysymToKeycode)(self.display, sym) };
        NonZeroU8::new(kc)
    }
    pub fn keycode_to_keysym(&self, co: u8, idx: i32) -> Option<u64> {
        let kc = unsafe {  (self.xlib().XKeycodeToKeysym)(self.display, co, idx) };
        if kc == NoSymbol as u64 {
            return None
        }
        Some(kc)
    }

    fn pop_event(& self) ->   Option<Event> {
        let e = self.check_if_event(|_d, _e| true)?;
        let xe = self.xinput_dev_ev(&e);
        if xe.is_some(){
            return xe
        }
        Some(Event::newx(e))
    }
    fn xinput_dev_ev(&self, value: &XEvent) -> Option<Event>  {
        unsafe {
            let mut xcookie = *Self::generic_xin_ev(value, self.xiopcode)?;
            if (self.xlib().XGetEventData)(self.display, &mut xcookie) == 0 {
                error!("no XGetEventData");
                return None
            };
            if xcookie.evtype ==  XI_KeyPress || xcookie.evtype == XI_KeyRelease {
                return Some(Event::XIDev(DeviceEvent::new(xcookie)))
            }
            match xcookie.evtype {
                XI_HierarchyChanged  => {
                    return Some(Event::XIHierarchy(HierarchyEvent::new(xcookie)));
                }
                _ => {
                }
            };
            let d = match xcookie.evtype {
                XI_DeviceChanged  => Some(Event::XIDeviceChange(*(xcookie.data as *mut _))),
                _ => {
                    error!("other generic_event_cookie.evtype {}",  xcookie.evtype);
                    None
                }
            };

            (self.xlib().XFreeEventData)(self.display, &mut xcookie);
            d
        }
    }
    fn generic_xin_ev( value: &XEvent, opco: i32) -> Option<&xlib::XGenericEventCookie>  {
        unsafe {
            if value.type_ != xlib::GenericEvent {
                return None
            }
            let xcookie: &xlib::XGenericEventCookie = &value.generic_event_cookie;
            if xcookie.extension != opco {
                return None
            }
            Some(xcookie)
        }
    }

    fn pop_event_repress(& self, kc: u32, time: u64) ->   Option<XKeyEvent> {
        let e = self.check_if_event(|_d, e: &XEvent| {
            if e.get_type() ==  xlib::KeyPress {
                let ke = unsafe {
                    (*e).key
                };
                ke.keycode == kc && time.saturating_sub( ke.time) < 5
            } else {
                false
            }
        })?;
        Some(unsafe { e.key })
    }
    fn check_if_event<F: FnMut (*mut _XDisplay, & XEvent) -> bool>(& self, mut pred: F) ->   Option<XEvent> {
        struct Fna<'a> {
            f: &'a mut dyn FnMut (*mut _XDisplay, & XEvent) -> bool,
        }
        extern "C"  fn cb(
            display: *mut xlib::Display,
            e: *mut XEvent,
            arg: *mut i8,
        ) -> i32 {
            let a = arg as *mut Fna;
            let fa = unsafe { &mut *a };
            let e = unsafe { &*e };
            (fa.f)(display, e) as _
        }
        let mut xevent = MaybeUninit::uninit();
        let mut fa = Fna{  f: &mut pred };
        unsafe {
            if (self.xlib().XCheckIfEvent)(
                self.display,
                xevent.as_mut_ptr(),
                Some(cb),
                (&mut fa) as *mut _ as *mut _,
            ) == 0 {
                return None
            }
            Some(xevent.assume_init())
        }
    }
    #[allow(dead_code)]
    pub fn ext_list(&self) -> Extensions {
        let mut n = 0;
        unsafe {
            let ls: *mut *mut i8 =  (self.xlib().XListExtensions)(self.display, &mut n as *mut _ as *mut _);
            let lsl = &mut * std::ptr::slice_from_raw_parts_mut(ls, n);
            Extensions { exts: lsl, xlib: self.xlib() }
        }
    }
    pub fn query_extension(&self, nm: &CStr) -> Option<(i32, i32, i32)> {
        query_extension(self.xlib(), self.display, nm)
    }
    pub fn query_keys_down(&self) -> impl Iterator<Item = u8> {
        let mut ks = [0; 32];
        unsafe {
            let _ =  (self.xlib().XQueryKeymap)(self.display,ks.as_mut_ptr());
        }
        bits_to_poss(ks)
    }

    pub fn send_event(&self, win: u64, mask: i64, ev: &mut XEvent) -> i32 {
        let v= unsafe { (self.xlib().XSendEvent)(self.display, win, true as _, mask,  ev as *mut _  )};
        self.xdp.flush();
        v
    }
    #[allow(dead_code)]
    pub fn send_key_event(&self, win: u64, keycode: u32, mask: u32, press: bool) -> i32 {
        let ke = XKeyEvent{
            type_: if press { KeyPress}  else {KeyRelease},
            serial: 0, send_event: 0, display: self.display,
            window: win, root: 0, subwindow: 0, time: CurrentTime, x: 1, y: 1, x_root: 1, y_root: 1, state: mask,
            keycode,
            same_screen: true as _ };
        let mut ke = ke.into();
        self.send_event(win, KeyPressMask, &mut ke)
    }
    #[allow(dead_code)]
    pub fn focused_win(&self, ) -> u64  {
        let mut w = 0u64;
        unsafe{
            (self.xlib().XGetInputFocus)(self.display, &mut w as *mut _, &mut 0 as *mut _);
        }
        w
    }
}
impl XlibDpy {
    pub fn flush(&self) -> i32 {
        unsafe {
            (self.xlib.XFlush)(self.display)
        }
    }
    pub fn sync(&self) -> i32 {
        unsafe {
            (self.xlib.XSync)(self.display, false as _)
        }
    }

    fn disp_keycodes(&self) -> Result<(i32, i32), XlibErr > {
        let mut b = 0;
        let mut e = 0;
        x_err(unsafe{ (self.xlib.XDisplayKeycodes)(self.display, &mut b as *mut _, &mut e as *mut _,)})?;
        Ok((b, e))
    }
    /// keycodes
    /// keysyms of each keycode
    pub fn codes_syms(&self) -> Result<SymsOfCodes, XlibErr >  {
        let ( first, last) = self.disp_keycodes()?;
        debug!("first, {first}, last {last}");
        let cnt = last-first+1;
        let mut syms_each = 0;
        let syms: *mut KeySym =  unsafe{(self.xlib.XGetKeyboardMapping)(self.display, first as u8, cnt,  &mut syms_each as *mut _)};
        let totl = cnt * syms_each;
        let lsl = unsafe{
            XOwnSlice::new(syms, totl as usize )
        };
        Ok(SymsOfCodes{ syms: lsl, each: syms_each as usize , last: last as u8,  })
    }
    pub fn change_key_mapping(&self, keycode: u8, sym: u64)  -> Result<(), XlibErr >  {
        let mut syms= [sym];
        x_expect(0, unsafe{
            (self.xlib.XChangeKeyboardMapping)(self.display, keycode as i32, 1, &mut syms as *mut _, 1)
        })?;
        Ok(())
    }
    #[allow(dead_code)]
    pub fn kb_state(&self )  -> Result<xlib::_XkbStateRec, XlibErr> {
        let mut state = MaybeUninit::uninit();
        unsafe{
            x_err_o(
                (self.xlib.XkbGetState)(self.display, XKB_USE_CORE_KBD, state.as_mut_ptr())
            , 0)?;
            Ok(state.assume_init())
        }
    }
    pub fn modifier_codes(&self) -> ModifierCodes  {
        let syms: *mut xlib::XModifierKeymap =  unsafe{(self.xlib.XGetModifierMapping)(self.display)};
        let lsl = unsafe{
            let w = (* syms).max_keypermod;
            let ksp = (* syms).modifiermap;
            & * std::ptr::slice_from_raw_parts(ksp, w as usize * 8)
        };
        ModifierCodes{ modm: syms, keycodes: lsl, xlib: &self.xlib }
    }
    /// convert name
    pub fn string_to_keysym(&self, nm: &CStr) -> Option<NonZeroU64>  {
        let pt = unsafe{
            (self.xlib.XStringToKeysym)(nm.as_ptr())
        };
        if pt == NoSymbol as u64 {
            None
        } else {
            NonZeroU64::new(pt)
        }
    }
}

pub fn keysym_to_string(sym: u64) -> Option<&'static CStr>  {
    unsafe{
        let pt = with_xl(|x| {
            let xl = &x.unwrap().xlib;
            (xl.XKeysymToString)(sym)
        });
        if pt.is_null() {
            return None
        }
        Some(CStr::from_ptr(pt))
    }
}

impl Xdll
{
    #[allow(dead_code)]
    fn creat_win_simp(&self) -> u64 {
        unsafe{
            (self.xlib().XCreateSimpleWindow)(self.display, self.rootwin, 5, 5, 9, 9, 0, 0xff0000ff, 0x00ff00ff)
        }
    }
    #[allow(dead_code)]
    fn map_win(&self, w: u64) -> Result<(), XlibErr>  {
        unsafe{
            x_expect(1, (self.xlib().XMapWindow)(self.display, w))
        }
    }

    #[allow(dead_code)]
    pub fn get_atom_name(&self, atom: u64) -> XOwnStr  {
        unsafe {
            let p= (self.xlib().XGetAtomName)(self.display, atom);
            XOwnStr::new(p, )
        }
    }
    pub fn intern_atom(&self, nm: &CStr) -> u64  {
        unsafe {
            (self.xlib().XInternAtom)(self.display, nm.as_ptr(), 1)
        }
    }
}

fn bits_to_poss(bits: [i8; 32]) -> impl Iterator<Item = u8> {
    bits.into_iter().enumerate().flat_map(|(i, kbs)| {
        let min = i as u8 *8;
        let mut kbs = kbs as u8;
        (0..8).map_while(move |_| {
            if kbs == 0 {
                return None
            }
            let b1p = kbs.trailing_zeros() as u8;
            kbs ^= (1 << b1p);
            Some(b1p + min)
        })
    })
}

pub fn query_extension(xlib: &Xlib, display: *mut _XDisplay, nm: &CStr) -> Option<(i32, i32, i32)> {
    let ns = nm.as_ptr();
    let mut mop = 0;
    let mut ev0 = 0;
    let mut err0 = 0;
    let t = unsafe { (xlib.XQueryExtension)(display, ns, &mut mop as *mut _, &mut ev0 as *mut _,  &mut err0 as *mut _,  )} != 0;
    assert_eq!(t, mop!= 0);
    Some((mop, ev0, err0))
}


#[derive(Debug)]
pub enum Event {
    /// no device information
    Key(KeyEvent),
    Mapping(XMappingEvent),
    XIDev(DeviceEvent),
    XIDeviceChange(XIDeviceChangedEvent),
    XIHierarchy(HierarchyEvent),
    Quit,
    Other(XEvent),
}


impl Event {
    fn newx(value: XEvent) -> Self {
        unsafe {
            match  value.type_ {
                xlib::KeyPress | xlib::KeyRelease => Self::Key(KeyEvent { x: value.key }),
                xlib::MappingNotify => Self::Mapping(value.mapping),
                _ => Event::Other(value),
            }
        }
    }
}

#[derive(Debug)]
pub struct KeyEvent {
    x: XKeyEvent,
}

impl KeyEvent {
    pub fn is_press(&self) -> bool {
        self.x.type_ == xlib::KeyPress
    }
}

pub struct SymsOfCodes{
    syms: XOwnSlice< u64>,
    each: usize ,
    last: u8,
}

impl<'a>   SymsOfCodes {
    pub fn iter(&self)-> impl DoubleEndedIterator<Item = (u8, &[KeySym])>  {
        let n = self.syms.len() / self.each;
        trace!("{n} key codes");
        let first = self.last - n as u8 + 1;
        (first..=self.last).zip((0..n).map(|i| {
            let b = i*self.each;
            let ss = &self.syms[b..b+self.each];
            let e = ss.iter().position(|&s| s == NoSymbol as KeySym).unwrap_or(self.each);
            &ss[..e]
        }))
    }
    /// keysym to keycode
    pub fn sym_key_map(&self) -> BTreeMap<u32, u8> {
        let mut m = BTreeMap::new();
        for (k, syms) in self.iter() {
            let Some(s) = syms.first() else { continue; };
            // the definitions uses c_uint
            let s = *s as u32;
            if m.contains_key(&s) { continue;}
            m.insert(s, k);
        }
        m
    }
    /// whether it's shifted
    pub fn sym_key_code(&self) -> BTreeMap<u32, (u8, bool )> {
        let mut m = BTreeMap::new();
        for (k, syms) in self.iter() {
            let Some(s) = syms.first() else { continue; };
            // the definitions uses c_uint
            let s = *s as u32;
            m.entry(s).or_insert((k, false ));
            let Some(s) = syms.get(1) else { continue; };
            let s = *s as u32;
            m.entry(s).or_insert((k, true  ));
        }
        m
    }
    /// codes not in use
    pub fn unassigned(&self) ->  impl Iterator<Item = u8> + '_ {
        let c = self
            .iter()
            .rev()
            .filter_map(|(c, syms)| {
            if syms.is_empty() { Some(c)} else {None}
            });
        c
    }
    pub fn fallback_unuse(&self) -> u8 {
        for (c, s) in self .iter() .rev(){
            let Some(s) = s.first().copied() else { return c };
            if s > 0x0100_0000 && s < 0x1000_0000 {
                return c
            }
        }
        252
    }
}

pub struct ModifierCodes<'a>{
    modm: *mut xlib::XModifierKeymap,
    keycodes: &'a [KeyCode],
    xlib: &'a Xlib,
}

impl<'a> std::fmt::Debug for ModifierCodes<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModifierCodes")
            .field("Shift", &self.index(0))
            .field("Lock", &&self[1]) // Caps
            .field("Control", &&self[2])
            .field("Mod1", &&self[3]) // alt meta
            .field("Mod2", &&self[4]) // num
            .field("Mod3", &&self[5])
            .field("Mod4", &&self[6]) // super hyper
            .field("Mod5", &&self[7]) // ISO_Level3_Shift Mode_switch
            .finish()
    }
}

impl<'a> Index<usize> for ModifierCodes<'a> {
    type Output= [KeyCode];

    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < 8);
        let kpm = self.max_key_per_mod();
        let beg = kpm * index;
        let s = &self.keycodes[beg.. beg + kpm];
        let n = s.iter().take_while(|&&c| c != 0).count();
        &s[..n]
    }
}


impl<'a>   ModifierCodes<'a> {
    pub fn max_key_per_mod(&self)->  usize {
        self.keycodes.len() / 8
    }
    pub fn shift(&self) -> u8 {
        *self.keycodes.first().expect("shift")
    }
    pub fn iter(&self)->impl Iterator<Item = KeyCode> + '_ {
        (0..).map(|i| i*self.max_key_per_mod())
            .take_while(|&b| b<self.keycodes.len())
            .flat_map(|beg| 
            self.keycodes[beg..].iter().take(8).copied() .take_while(|&c| c!= 0)
        )
    }

    pub fn code_map(&self) -> HashMap<u8, u32> {
        let mut map = HashMap::new();
        for i in 0..8 {
            let modbit = 1 << i;
            for k in &self[i as _] {
                map.insert(*k , modbit);
            }
        }
        map
    }
}

impl<'a> Drop for ModifierCodes<'a> {

    fn drop(&mut self) {
        unsafe {
            (self.xlib.XFreeModifiermap)(self.modm);
        }
    }
}

pub struct Extensions<'a> {
    exts: &'a mut [*mut i8],
    xlib: &'a Xlib,
}

impl<'a> Index<usize> for   Extensions<'a> {
    type Output= CStr;

    fn index(&self, index: usize ) -> &Self::Output {
        assert!(index < self.exts.len());
        let p = self.exts[index];
        let s = unsafe {  CStr::from_ptr(p)};
        s
    }
}

impl<'a> Drop for   Extensions<'a> {
    fn drop(&mut self) {
        let _ =unsafe {
            (self.xlib.XFreeExtensionList)(self.exts.as_mut_ptr())
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;
    #[test]
    fn size1() {
        assert_eq!(192, mem::size_of::<XEvent>());
    }
     #[test]
    fn bps() {
        let v : Vec<u8> = bits_to_poss([0; 32]).collect();
        assert!(v.is_empty());
        let mut b = [0;32];
        b[0]=1;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [0]);
        let mut b = [0;32];
        b[0]=2;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [1]);
        let mut b = [0;32];
        b[0]=3;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [0, 1]);
    }
    #[test]
    fn bps1() {
        let mut b = [0;32];
        b[0]=4;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [2]);
        let mut b = [0;32];
        b[0]=6;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [1,2]);
        let mut b = [0;32];
        b[0]=20;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [2, 4]);
    }
    #[test]
    fn bps2() {
        let mut b = [0;32];
        b[3]=4;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [26]);
        let mut b = [0;32];
        b[3]=6;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [25,26]);
        let mut b = [0;32];
        b[3]=20;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [26, 28]);
        let mut b = [0;32];
        b[3]=20;
        b[4]=9;
        let v : Vec<u8> = bits_to_poss(b).collect();
        assert_eq!(v, [26, 28, 32, 35]);
    }
}
