use std::collections::VecDeque;
use std::process;

use std::time::{Instant, Duration};






mod key_changer;

use crate::conf::{Action};
use crate::xdl::{Xtestdl, with_xl};

use self::key_changer::SymCode;

pub struct  Xdo{
    xt: Xtestdl,
    acts : VecDeque<Action<u32>>,
    last_add : Vec<Action<u32>>,
    replay: VecDeque<(u8, bool)>,
    resume: Instant,
    acting: Option<RunAct>,
    sym_code: SymCode,
}

impl Xdo {
    pub fn new(xt: Xtestdl) -> Self {
        Self { xt,
               acts: VecDeque::new(),
               last_add: vec!(),
               replay: VecDeque::new(),
               resume: Instant::now(),
               acting: None,
               sym_code: SymCode::new().unwrap(),
        }
    }
    /// keyboard plugged or unplugged
    pub fn ondevchange(&self) {
        self.sym_code.affirm();
        with_xl(|r| {
            let x = r.unwrap();
            x.flush();
        });
    }
    pub fn pass_key(&self, code: u8, press: bool ) {
        debug!("pass {} {}", code, press);
        self.xt.fake_dev_key(code as u32, press);
    }
    pub fn  add_acts(&mut self, acts: &[Action<u32>]) {
        let mut iter = acts.iter();
        let Some(first) = iter.next() else {
            return
        };
        if let Action::Repeat(n) = first {
            info!("will repeat {n} times {:?}", self.last_add);
            for _ in 0..*n {
                self.acts.extend(self.last_add.iter().cloned());
            }
        } else {
            self.acts.push_back(first.clone());
            self.last_add.clear();
            self.last_add.extend(acts.iter().cloned());
        }
        for t in iter {
            self.acts.push_back(t.clone());
        }
    }
    pub fn add_unmatch(&mut self, seq: &[(u8, bool)]){
        if seq.len() >8 {
            warn!("too many keys {seq:?}");
            return;
        }
        self.replay.extend(seq);
    }
    pub fn proc(&mut self) -> Option<Duration> {
        let now = Instant::now();
        if now < self.resume {
            return Some(self.resume - now)
        }
        if let Some(ref mut act) = self.acting {
            if let Some(sleep)=act.proc(&self.xt, &mut self.sym_code){
                self.resume = now + sleep;
                return Some(sleep)
            } else {
                self.acting = None;
            }
        }
        if let Some((c, p))= self.replay.pop_front() {
            let sl = Duration::from_millis(3);
            self.resume = now + sl;
            debug!("k {c} d={p}");
            self.xt.fake_dev_key(c as _, p);
            return Some(sl)
        }
        while let Some(action) = self.acts.pop_front(){
            if let Some(mut a)=sing(action) {
                if let Some(s) = a.proc(&self.xt,&mut self.sym_code) {
                    self.acting = Some(a);
                    return Some(s)
                } else {
                    info!("finished");
                }
            } else {
                info!("done");
            }
        }
        None
    }
}

enum RunAct {
    Txt{
        content: String,
        index: usize ,
        pressing: bool ,
        shifted:bool ,
    },
    Keys{
        keys: Vec<u32>,
        index: usize ,
        pressing: bool ,
    },
    MouseClick {
        btn: u32,
        pressing: bool ,
    },
}

impl RunAct {
    fn proc(&mut self, xts: &Xtestdl, syc: &mut SymCode) -> Option<Duration>  {
        match self{
            RunAct::Txt { content: t, index, pressing, shifted } => {
                let i = *index;
                if *index >= t.len() {
                    return None
                }
                let charend = ((i+1)..t.len()).find(|&i|t.is_char_boundary(i) ).unwrap_or(t.len());
                let chs = &t[i..charend];
                if chs.is_empty(){
                    return None
                }
                let (kc, g) =syc.find_sym_str(chs);
                if g.is_new() {
                    return Some(Duration::from_millis(1))
                }
                if *pressing {
                    if g.is_shift() && !*shifted {
                        xts.fake_dev_key(syc.shift_key() as _, *pressing);
                        *shifted =true ;
                        return Some(Duration::from_millis(2))
                    }
                    xts.fake_dev_key(kc as _, *pressing);
                    debug!("down {}", chs);
                    *pressing = false ;
                    Some(Duration::from_millis(5))
                } else {
                    if g.is_shift() && *shifted {
                        xts.fake_dev_key(syc.shift_key() as _, false );
                        *shifted =false  ;
                        return Some(Duration::from_millis(2))
                    }
                    debug!("up {}", chs);
                    xts.fake_dev_key(kc as _, false );
                    *pressing = true  ;
                    *index = charend;
                    Some(Duration::from_millis(12))
                }
            }
            RunAct::Keys { keys: ks, index, pressing } => {
                let keysym = match ks.get(*index) {
                    Some(k) => k,
                    None if *pressing => {
                        *pressing = false ;
                        *index = 0;
                        ks.first()?
                    }
                    None => return None
                };
                let (kc, g)= syc.find_sym(*keysym);
                if g.is_new() {
                    return Some(Duration::from_millis(1))
                }
                *index += 1;
                xts.fake_dev_key(kc as _, *pressing);
                debug!("dk {keysym:x} kc={kc} pr={}",pressing);
                Some(Duration::from_millis(2))
            }
            RunAct::MouseClick{ btn, pressing } => {
                xts.fake_btn(*btn, *pressing);
                info!("MouseClick {btn} d={}", pressing);
                if *pressing {
                    *pressing = false ;
                    Some(Duration::from_millis(4))
                } else {
                    None
                }
            }
        }
    }
}

fn sing(c: Action<u32>) -> Option<RunAct>  {
    Some(match c {
        Action::Text(t) => {
            RunAct::Txt { content: t, index: 0, pressing: true , shifted: false  }
        }
        Action::KeyStroke(ks) => {
            RunAct::Keys { keys: ks, index: 0, pressing: true }
        }
        Action::MouseClick(c) => {
            RunAct::MouseClick { btn: c as _, pressing: true  }
        }
        Action::Repeat(_) => {
            error!("unexpectd");
            return None
        }
        Action::Exec(e) => {
            info!("running {e:?}");
            let mut gs = e.into_iter();
            match process::Command::new(gs.next()?).args(gs).spawn(){
                Ok(_o) => {
                }
                Err(e) => error!("spawn Command fail {e:?}"),
            }
            return None
        }
    })
}
