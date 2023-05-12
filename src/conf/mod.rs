use std::{collections::{HashMap, BTreeMap, HashSet}, num::NonZeroU32, ffi::CString, fmt::Debug};



use crate::{xdl::{Xdll, keysym::ALIAS_KEYSYM, with_xl}, keysym_to_string};

use self::{fs::read_seq_file, parse::assignment_line};

mod parse;
mod fs;

#[derive(PartialEq, Debug, Clone)]
pub enum Action<Key>{
    Text(String),
    /// key stroke combination
    KeyStroke(Vec<Key>),
    MouseClick(u8),
    Repeat(u8),
    /// spawn process
    Exec(Vec<String>),
}

struct MapBuilder {
    grabkeys: BTreeMap<u8, HashSet<u32>>,
    // collect sequence to action mapping
    map: HashMap<SmoVec<u8>, Vec<Action<u32>>>,
    trans: TransKeyName,
    symbuf: Vec<Option<NonZeroU32>>,
    sym_to_code: BTreeMap<u32, u8>,
    code_sym: HashMap<u8 ,u32 >,
    keycode_mod: HashMap<u8 ,u32 >,
}

fn beginning(kseq: &[u8], keycode_mod: &HashMap<u8 ,u32 >,)-> Option<(u8, u32)> {
    let mut mo = 0;
    for k in kseq.iter() {
        if let Some(m) = keycode_mod.get(k) {
            mo ^= m;
        }else {
            return Some((*k, mo))
        }
    }
    None
}

impl<'a> MapBuilder {
    fn new() -> Self {
        let (sym_to_code, keytomo) = with_xl(|r|{
            let x = r.unwrap();
            let keytomo = x.modifier_codes().code_map();
            (x.codes_syms().expect("getting range of keycodes").sym_key_map(), keytomo)
        });
        let code_sym =sym_to_code.iter().map(|(&s, &c)| (c, s)).collect();
        // convert name of key to u32
        let tsk = TransKeyName::new();
        Self {  trans: tsk, grabkeys: BTreeMap::new(), map: Default::default(), symbuf: vec!(), sym_to_code, code_sym,
                keycode_mod:keytomo
        }
    }
    fn add (&mut self, (sq, acts): (Vec<&str>, Vec<Action<String>>), lineind: i32){
        if sq.len() > 16 {
            error!("Line {lineind}, seq  too long {:?}", sq);
            return
        }
        self.trans.conv_seq(&sq, &mut self.symbuf);
        if let Some(i) = self.symbuf.iter().position(|sym| sym.is_none()) {
            error!("Line {lineind}, unrecognized key {} in {sq:?}", sq[i]);
            return
        }
        let mut seq_codes = [0; 16];
        for (sym, i) in self.symbuf.iter().zip(0..16) {
            let sym = sym.unwrap().get();
            self.trans.sym_name.conv(sym);
            let cd = self.sym_to_code.get(&sym).unwrap_or(&0);
            seq_codes[i] = * cd;
        }
        let seq_codes: &[u8] = &seq_codes[..self.symbuf.len()];
        for (i, &c) in seq_codes.iter().enumerate(){
            if c == 0 {
                error!("Cannot find key {}, keySym={}", sq[i], seq_codes[i]);
                return
            }
        }
        if let Some((k, m)) = beginning(seq_codes, &self.keycode_mod) {
            self.grabkeys.entry(k).or_default().insert(m);
        };
        let mut atsn: Vec<Action<u32>> = Vec::with_capacity(acts.len());
        for a in acts {
            let Some(a) =a.trans_key(|n| self.trans.get_keysym(n).map(|s| s.get()) )  else {
                return
            };
            atsn.push(a);
        }
        let Some(_sym) = self.symbuf.first()  else {
            error!("Line {lineind}, seq empty");
            return
        };
        let Some(ks) = SmoVec::new(seq_codes) else { return };
        let acdisp = DispActs { acts:  &atsn, sym_name:  &self.trans.sym_name.0};
        if let Some(v) =self.map.remove(&ks) {
            warn!("{:?} already assigned to {:?}, replacing with {:?}", sq, v, acdisp);
        } else {
            info!("Map: {:?} ⇒ {:?}", DispSeq{ sq: ks.slice(), code_sym: &self.code_sym, sym_name: &self.trans.sym_name.0 }, acdisp);
        }
        self.map.insert(ks, atsn);
    }
}

pub(crate) struct DispActs<'a> {
    pub(crate) acts: &'a Vec<Action<u32>>,
    pub(crate) sym_name: &'a HashMap<u32 , String>,
}


impl<'a> std::fmt::Debug for DispActs<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut p = false;
        for (a) in self.acts.iter() {
            if p {
                write!(f, ", ")?;
            }
            p = true;
            match a {
                Action::Text(t) => write!(f, "text {t:?}")?,
                Action::KeyStroke(kc) => {
                    let mut pk = false;
                    write!(f, "key ")?;
                    for sym in kc {
                        if pk {
                            write!(f, "+")?;
                        }
                        pk = true;
                        let na = self.sym_name.get(sym);
                        if let Some(n) = na {
                            write!(f, "{n}")?;
                        }else{
                            write!(f, "{sym:#x}")?;
                        }
                    }
                }
                Action::MouseClick(mb) => write!(f, "mouse {mb}")?,
                Action::Repeat(n) => write!(f, "repeat {n}")?,
                Action::Exec(x) => write!(f, "exec {x:?}")?,
            }
        }
        Ok(())
    }
}

pub(crate) struct DispSeq<'a>{
    pub(crate) sq: &'a [u8],
    pub(crate) code_sym: &'a HashMap<u8 ,u32>,
    pub(crate) sym_name: &'a HashMap<u32 , String>,
}

impl<'a> std::fmt::Debug for DispSeq<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut p = false;
        for (i, k) in self.sq.iter().enumerate() {
            if p {
                write!(f, " ")?;
            }
            p = true;
            let sym = self.code_sym.get(k);
            let na = sym.and_then(|s| self.sym_name.get(s));
            if let Some(n) = na {
                write!(f, "{n}")?;
            }else if let Some(s) = sym {
                write!(f, "{s:#x}")?;
            }else {
                write!(f, "{k}")?;
            }
            let a = if (&self.sq[..i].iter().filter(|&a| a == k).count() %2 ==0) { "↘"}else {"↗"};
            write!(f, "{}", a)?;
        }
        Ok(())
    }
}

pub struct Mapping {
    grabs: BTreeMap<u8, HashSet<u32>>,
    seq_act:  HashMap<SmoVec<u8>, Vec<Action<u32>>>,
    // for display
    pub(crate) code_sym: HashMap<u8 ,u32 >,
    pub(crate) sym_name: HashMap<u32, String>,
}

impl Mapping {
    pub fn get(&self, v: &SmoVec<u8>) -> Option<&Vec<Action<u32>>> {
        self.seq_act.get(v)
    }
    pub fn setup_device(&self, dev: i32, x: &crate::Xconn)-> Result<(), Box<dyn std::error::Error>> {
        x.select_dev_events(dev)?;
        for (&key, mods) in self.grabs.iter() {
            for &m in mods.iter() {
                let sym = self.code_sym.get(&key).copied().unwrap_or_default();
                let nm = self.sym_name.get(&sym).cloned().unwrap_or_default();
                debug!("grb k {nm} mod {m}");
                if let Err(_e)= x.grab_dev_key(dev, key as i32, [m]){
                    error!("Key {nm} unavailable for exclusive grabbing keyCode={key} device={dev}");
                    return Err(format!("key {nm} unavailable").into());
                }
            }
        }
        Ok(())
    }
}
pub fn load_mapping(_xd: & Xdll)-> Result<Mapping, std::io::Error>{
    let mut build =MapBuilder::new();
    let s = read_seq_file()?;
    for (l, lineind) in s.lines().zip(1..) {
        match assignment_line(l) {
            Ok((input, a)) => if let Some((sqa)) = a {
                build.add(sqa, lineind);
            } else if !input.is_empty() {
                info!("parsed no mapping in {}.", input);
            }
            Err(e) => {
                warn!("could not parse line {lineind} \"{l}\", error: {e:?}");
            }
        }
    }
    let m = Mapping { grabs: build.grabkeys, seq_act: build.map,
                      code_sym: build.code_sym,
                      sym_name:  build.trans.sym_name.0
    };
    Ok(m)
}

impl Action<String> {
    /// convert key name to sym
    /// to check validity of conf
    fn trans_key<F: FnMut(&str)-> Option<u32> >(self, mut tf: F)->Option< Action<u32>> {
        Some(
        match self {
            Action::KeyStroke(vs) => {
                let mut v = Vec::with_capacity(vs.len());
                for kn in &vs{
                    let n: &str = kn;
                    let Some(ks) = tf(n)  else {
                        error!("key {} in {:?} is not recognized", n, vs);
                        return None
                    };
                    v.push(ks);
                }
                Action::KeyStroke(v)
            },
            Action::Text(t) => Action::Text(t),
            Action::MouseClick(x) => Action::MouseClick(x),
            Action::Repeat(x) => Action::Repeat(x),
            Action::Exec(x) => Action::Exec(x),
        })
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub enum SmoVec<T>{
    Vec2([T; 2]),
    Vec4([T; 4]),
    Vec6([T; 6]),
    Vec8([T; 8]),
    VecM(Vec<T>),
}

impl<T: Copy+Default> SmoVec<T> {
    pub fn new(s: &[T])-> Option<Self> {
        Self::from_iter(s.iter().copied())
    }
    pub fn from_iter<I: IntoIterator<Item = T>>(es: I) -> Option<Self> {
        let mut es = es.into_iter();
        let mut i = 0;
        let mut v = [T::default(); 8];
        for x in v.iter_mut() {
            let Some(t) = es.next()  else {break;};
            *x = t;
            i += 1;
        }
        if let Some(x) = es.next() {
            let mut v = v.to_vec();
            v.push(x);
            v.extend(es);
            return Some(Self::VecM(v))
        }
        Some(match i {
            2 => Self::Vec2([v[0], v[1]]),
            4 => Self::Vec4(v.split_at(4).0.try_into().unwrap()),
            6 => Self::Vec6(v.split_at(6).0.try_into().unwrap()),
            8 => Self::Vec8(v.split_at(8).0.try_into().unwrap()),
            _ => return None
        })
    }
    #[allow(dead_code)]
    pub fn slice(&self) -> &[T] {
        match self{
            SmoVec::Vec2(s) => s,
            SmoVec::Vec4(s) => s,
            SmoVec::Vec6(s) => s,
            SmoVec::Vec8(s) => s,
            SmoVec::VecM(s) => s,
        }
    }
}

struct TransKeyName {
    name_sym: HashMap<String, NonZeroU32>,
    sym_name: SymToName,
}

#[derive(Default)]
struct SymToName(HashMap<u32, String>);

impl SymToName {
    fn conv(&mut self, sym: u32) -> Option<&str> {
        if !self.0.contains_key(&sym) {
            let x = keysym_to_string(sym as _);
            let n = x?.to_str().ok()?.to_string();
            self.0.insert(sym , n, );
        }
        self.0.get(&sym).map(|x| &**x)
    }
}

impl<'a> TransKeyName {
    fn new() -> Self {
        Self {
            name_sym: Default::default(),
            sym_name: Default::default(),
        } }

    /// translate keys
    fn conv_seq(&mut self, names: &[&str], buf: &mut Vec<Option<NonZeroU32>>) {
        buf.resize(0, None);
        for &n in names {
            buf.push(self.get_keysym(n));
        }
    }
    /// translate name to KeySym, unsigned number
    fn get_keysym(&mut self, name: &str)-> Option<NonZeroU32> {
        let sym = self.name_sym.get(name).copied();
        if sym.is_some() {
            return sym
        }
        let mut chas = name.chars();
        let mut sym = None;
        if let (Some(c), None) = ( chas.next(),  chas.next()) {
            if c.is_ascii_alphanumeric() {
                sym = NonZeroU32::new(c.to_ascii_lowercase() as u32);
            }
        }
        if sym.is_none() {
            let s = CString::new(name).ok()?;
            let sm =with_xl(|x|{
                x.unwrap().string_to_keysym(&s)
            });
            if let Some(sm) = sm {
                sym = NonZeroU32::new( sm.get() as u32);
            }
        }
        if sym.is_none() {
            if let Some(&(_0, sy))= ALIAS_KEYSYM.iter().find(|(n, _1)| n.eq_ignore_ascii_case(name)){
                sym = NonZeroU32::new(sy);
            }
        }
        let sym = sym?;
        self.name_sym.insert(name.to_string(), sym);
        let name = keysym_to_string(sym.get() as _)?.to_str().ok()?;
        self.sym_name.0.insert(sym.get(), name.to_string());
        Some(sym)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t1() {
        let sv = SmoVec::from_iter((0..10)).unwrap();
        assert_eq!(sv.slice(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let sv = SmoVec::from_iter((0..6)).unwrap();
        assert_eq!(sv.slice(), [0, 1, 2, 3, 4, 5]);
        let sv = SmoVec::from_iter((0..8)).unwrap();
        assert_eq!(sv.slice(), [0, 1, 2, 3, 4, 5, 6, 7]);
        let sv = SmoVec::from_iter((0..7));
        assert!(sv.is_none(),);
        let s = std::mem::size_of::<SmoVec<u8>>();
        assert_eq!(s, 32);
    }
}
