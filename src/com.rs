use winapi::shared::winerror::FAILED;
use winapi::um::unknwnbase::IUnknown;
use winapi::Interface;

use std::hash::{Hash, Hasher};
use std::mem;
use std::ops::Deref;
use std::ptr::{null_mut, NonNull};

#[repr(transparent)]
pub struct ComPtr<T>(NonNull<T>);

impl<T> ComPtr<T> {
    pub unsafe fn from_ptr(ptr: *mut T) -> Self
    where
        T: Interface,
    {
        ComPtr(NonNull::new(ptr).expect("ptr should not be null"))
    }

    pub fn as_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    pub fn into_ptr(self) -> *mut T {
        let p = self.0.as_ptr();
        mem::forget(self);
        p
    }

    fn as_unknown(&self) -> &IUnknown {
        unsafe { &*(self.0.as_ptr() as *mut IUnknown) }
    }

    pub fn up<U>(self) -> ComPtr<U>
    where
        T: Deref<Target = U>,
        U: Interface,
    {
        unsafe { ComPtr::from_ptr(self.into_ptr() as *mut U) }
    }

    // Cast creates a new ComPtr requiring explicit destroy call to avoid memory leaks.
    pub fn cast<U>(&self) -> Result<ComPtr<U>, i32>
    where
        U: Interface,
    {
        let mut ptr = null_mut();
        let hr = unsafe { self.as_unknown().QueryInterface(&U::uuidof(), &mut ptr) };
        if FAILED(hr) {
            return Err(hr);
        }
        Ok(unsafe { ComPtr::from_ptr(ptr as *mut U) })
    }
}

impl<T> Drop for ComPtr<T> {
    fn drop(&mut self) {
        unsafe {
            self.as_unknown().Release();
        }
    }
}

impl<T> Clone for ComPtr<T>
where
    T: Interface,
{
    fn clone(&self) -> Self {
        unsafe {
            self.as_unknown().AddRef();
            ComPtr::from_ptr(self.as_ptr())
        }
    }
}

impl<T> Deref for ComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
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
