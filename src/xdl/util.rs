use std::{ffi::{c_void, CStr}, ptr::{slice_from_raw_parts}, ops::Deref};

use x11_dl::xlib::Xlib;

use super::with_xl;

pub struct XOwnSlice<T> {
    ptr: *mut T,
    len: usize ,
}

impl<'a, T, O> PartialEq<O> for XOwnSlice<T>
where T: PartialEq, O: Deref<Target = [T]> {
    fn eq(&self, other: &O) -> bool {
        self == other
    }
}

impl<'a, T: std::fmt::Debug> std::fmt::Debug for XOwnSlice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.deref())
    }
}

impl<'a, T> XOwnSlice<T> {
    pub(crate) unsafe fn new( ptr: *mut T, len: usize ) -> Self { Self { ptr: ptr as * mut _, len } }
    pub fn slice(&self) -> &[T] {
        unsafe {
            &* slice_from_raw_parts(self.ptr, self.len)
        }
    }
}

impl<'a, T> Deref for XOwnSlice<T> {
    type Target=[T];

    fn deref(&self) -> &Self::Target {
        self.slice()
    }
}
impl<'a, T> Drop for XOwnSlice<T> {
    fn drop(&mut self) {
        with_xl(|x| {
            unsafe {
                (x.unwrap().xlib.XFree)(self.ptr as *mut _);
            }
        })
    }
}

pub struct XOwnStr {
    ptr: *mut i8,
}


impl<'a> std::fmt::Debug for XOwnStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.to_str() {
            Ok(s) => write!(f, "{:?}", s),
            Err(_) => write!(f, "{:?}", self.to_bytes()),
        }
    }
}

impl<'a> XOwnStr {
    pub unsafe fn new(ptr: *mut i8,) -> Self { Self { ptr,  } }
}

impl Deref for XOwnStr {
    type Target= CStr;

    fn deref(&self) -> &Self::Target {
        unsafe {CStr::from_ptr(self.ptr)}
    }
}


impl<'a> Drop for XOwnStr {
    fn drop(&mut self) {
        with_xl(|x| unsafe { (x.unwrap().xlib.XFree)(self.ptr as *mut _); })
    }
}

/// add as last field
pub(crate) struct XFreer<'a> {
    xlib: &'a Xlib,
    ptr: *mut c_void,
}

impl<'a> Drop for XFreer<'a> {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XFree)(self.ptr);
        }
    }
}

