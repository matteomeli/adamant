use winapi::ctypes::c_void;
use winapi::shared::winerror::FAILED;
use winapi::um::unknwnbase::IUnknown;
use winapi::Interface;

use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Deref;
use std::ptr;

#[repr(transparent)]
pub struct ComPtr<T>(*mut T);

impl<T> ComPtr<T> {
    pub fn empty() -> Self
    where
        T: Interface,
    {
        ComPtr(ptr::null_mut())
    }

    pub fn from_ptr(ptr: *mut T) -> Self
    where
        T: Interface,
    {
        ComPtr(ptr)
    }

    pub fn as_ptr(&self) -> *const T {
        self.0
    }

    pub fn as_ptr_mut(&self) -> *mut T {
        self.0
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    pub fn into_ptr(self) -> *mut T {
        let p = self.0;
        mem::forget(self);
        p
    }

    pub unsafe fn as_mut_void(&mut self) -> *mut *mut c_void {
        &mut self.0 as *mut *mut _ as *mut *mut _
    }

    fn as_unknown(&self) -> &IUnknown {
        debug_assert!(!self.is_null(), "ptr should not be null");
        unsafe { &*(self.as_ptr() as *mut IUnknown) }
    }

    pub fn up<U>(self) -> ComPtr<U>
    where
        T: Deref<Target = U>,
        U: Interface,
    {
        ComPtr::from_ptr(self.into_ptr() as *mut U)
    }

    // Cast creates a new ComPtr requiring explicit destroy call to avoid memory leaks.
    pub fn cast<U>(&self) -> Result<ComPtr<U>, i32>
    where
        U: Interface,
    {
        let mut p = ComPtr::<U>::empty();
        if !self.is_null() {
            let hr = unsafe {
                self.as_unknown()
                    .QueryInterface(&U::uuidof(), p.as_mut_void())
            };
            if FAILED(hr) {
                return Err(hr);
            }
        }
        Ok(p)
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

impl<T> Clone for ComPtr<T>
where
    T: Interface,
{
    fn clone(&self) -> Self {
        if !self.is_null() {
            unsafe {
                self.as_unknown().AddRef();
            }
        }
        ComPtr::from_ptr(self.as_ptr_mut())
    }
}

impl<T> Deref for ComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        assert!(!self.is_null(), "ptr should not be null");
        unsafe { &*self.as_ptr() }
    }
}

impl<T> std::fmt::Debug for ComPtr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ComPtr( ptr: {:?} )", self.as_ptr())
    }
}

impl<T> PartialEq<*mut T> for ComPtr<T> {
    fn eq(&self, other: &*mut T) -> bool {
        self.as_ptr() == *other
    }
}

impl<T> PartialEq for ComPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ptr() == other.as_ptr()
    }
}

impl<T> Hash for ComPtr<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ptr().hash(state);
    }
}
