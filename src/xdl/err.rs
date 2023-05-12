use std::{error::Error, fmt::Display};

use x11_dl::xlib::{BadAccess, BadValue, BadWindow};

#[derive(Debug)]
pub enum XlibErr{
    Access,
    Value,
    Window,
    Other,
}


impl Display for XlibErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for XlibErr {
}

pub fn x_err(v: i32) -> Result<(), XlibErr> {
    x_err_o(v, 1)
}


pub fn x_expect(ok: u8, v: i32) -> Result<(), XlibErr> {
    let v = v as u8;
    if v  == ok {
         return Ok(());
    }
    let e = match v {
        BadAccess => XlibErr::Access,
        BadValue  => XlibErr::Value,
        BadWindow  => XlibErr::Window,
        _ => {
            error!("xlib error {v}");
            XlibErr::Other
        }
    };
    Err(e)
}

pub fn x_err_o(v: i32, ok: u8) -> Result<(), XlibErr> {
    x_expect(ok, v)
}
