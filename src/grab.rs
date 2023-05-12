//
#[macro_use]
extern crate log;

use crate::xdl::Xconn;
use std::{time::Duration};

pub(crate) mod xdl;

use crate::xdl::Event;

fn main() {
    env_logger::builder()
        .write_style(env_logger::WriteStyle::Always)
        .filter_level(log::LevelFilter::Info)
        .format_module_path(false)
        .format_target(false)
        .parse_default_env()
        .init();
    let mut x = Xconn::new().expect("opening Xlib");
    x.grab_keyboard().unwrap();
    for _ in 0.. 9 {
        let e = x.recv_timeout (Some(Duration::from_secs(1)));
        if let Some(Event::Key(k)) = e{
            eprint!("{}{} ", k.code(), if k.is_press(){"↘"}else {"↗"});
        } else {
            dbg!(e);
        }
    }

}
