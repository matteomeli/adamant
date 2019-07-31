use winapi::ctypes::c_void;
use winapi::shared::winerror;
use winapi::um::unknwnbase::IUnknown;
use winapi::Interface;

use std::clone::Clone;
use std::fmt;
use std::mem::forget;
use std::ops::Deref;
use std::ptr;

#[repr(transparent)]
pub struct ComPtr<T>(*mut T);

impl<T> ComPtr<T> {
    pub fn null() -> Self {
        Self(ptr::null_mut())
    }

    pub unsafe fn from_raw(raw: *mut T) -> Self {
        Self(raw)
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    pub fn as_raw(&self) -> *mut T {
        self.0
    }

    pub fn into_raw(self) -> *mut T {
        let p = self.0;
        forget(self);
        p
    }

    pub unsafe fn as_mut_void(&mut self) -> *mut *mut c_void {
        &mut self.0 as *mut *mut _ as *mut *mut _
    }

    fn as_unknown(&self) -> &IUnknown {
        debug_assert!(!self.is_null());
        unsafe { &*(self.as_raw() as *mut IUnknown) }
    }
}

impl<T> ComPtr<T>
where
    T: Interface,
{
    pub fn into<U>(self) -> ComPtr<U>
    where
        U: Interface,
    {
        unsafe { ComPtr::from_raw(self.into_raw() as *mut U) }
    }

    pub unsafe fn cast<U>(&self) -> Result<ComPtr<U>, i32>
    where
        U: Interface,
    {
        let mut p = ComPtr::<U>::null();
        let hr = self
            .as_unknown()
            .QueryInterface(&U::uuidof(), p.as_mut_void());
        if winerror::SUCCEEDED(hr) {
            Ok(p)
        } else {
            Err(hr)
        }
    }
}

impl<T> Drop for ComPtr<T> {
    fn drop(&mut self) {
        if !self.is_null() {
            unsafe {
                self.as_unknown().Release();
            }
        }
    }
}

impl<T> Deref for ComPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        debug_assert!(!self.is_null());
        unsafe { &*self.as_raw() }
    }
}

impl<T> fmt::Debug for ComPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<T> Clone for ComPtr<T>
where
    T: Interface,
{
    fn clone(&self) -> Self {
        unsafe {
            debug_assert!(!self.is_null());
            ComPtr::from_raw(self.as_raw())
        }
    }
}
