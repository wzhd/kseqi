use std::{env, path::PathBuf, ffi::OsString, io::{self, Write}, fs};



fn confdir()-> io::Result<PathBuf> {
    let mut confd =  if let Some(cd)= env::var_os("XDG_CONFIG_HOME"){
        PathBuf::from(cd)
    } else {
        let mut home = PathBuf::from( env::var_os("HOME").unwrap_or_else(OsString::new ));
        home.push(".config");
        home
    };
    confd.push("kseqi");
    fs::create_dir_all(&confd)?;
    Ok(confd)
}

pub(crate) fn read_seq_file()-> io::Result<String> {
    let mut fp = confdir()?;
    fp.push("kseqi.conf");
    if !fp.is_file() {
        warn!("No config file at {}", fp.display());
        let mut f = fs::File::options().read(true).write(true).create_new(true).open(&fp)?;
        f.write_all(include_bytes!("../../seq.conf.example"))?;
    }
    let s = fs::read_to_string(&fp)?;
    Ok(s)
}
