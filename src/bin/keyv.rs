use std::{error::Error, collections::HashSet, io::Write};

use kseqi_desktop::{Xconn, Event, keysym_to_string};
use log::warn;
use x11_dl::xinput2::XIAllDevices;


/// print keyboard events
pub fn main()-> Result<(), Box<dyn Error>> {
    env_logger::builder()
        .write_style(env_logger::WriteStyle::Always)
        .filter_level(log::LevelFilter::Warn)
        .format_module_path(false)
        .format_target(false)
        .parse_default_env()
        .init();
    let mut down = HashSet::new();
    let mut x = Xconn::new()?;
    x.select_dev_events(XIAllDevices)?;
    let mut so = std::io::stdout().lock();
    loop {
        let Some(e) = ( x.recv_timeout(None)) else {
            continue;
        };
        let de = match e{
            Event::XIDev(e) => e,
            Event::Mapping(_e) => {
                continue;
            },
            Event::Quit => {
                println!("Exiting");
                break;
            }
            e => {
                warn!("other ev {e:?}");
                continue;
            }
        };
        let Some((k, p)) = ({de.get_key() }) else {continue;};
        if  p{down.insert(k)} else { down.remove(&k)};
        let s = x.keycode_to_keysym(k, 0).and_then(keysym_to_string).unwrap_or_default();
        let s = s.to_str().unwrap_or_default();
        print!("{}{} ", s, if  p{"↘"}else {"↗"});
        if down.is_empty() {
            println!();
        }else {
            let _ =so.flush();
        }
    }
    Ok(())
}
