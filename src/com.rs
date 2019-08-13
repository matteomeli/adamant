use winapi::ctypes::c_void;
use winapi::shared::winerror::FAILED;
use winapi::um::unknwnbase::IUnknown;
use winapi::Interface;

use std::fmt;
use std::ops::Deref;
use std::ptr;

// A thin and weak wrapper around ComPtr
#[repr(transparent)]
pub struct ComPtr<T>(*mut T);

impl<T> ComPtr<T> {
    pub fn null() -> Self {
        ComPtr(ptr::null_mut())
    }

    pub unsafe fn from_raw(raw: *mut T) -> Self {
        ComPtr(raw)
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn as_ptr(self) -> *const T {
        self.0
    }

    pub fn as_raw(self) -> *mut T {
        self.0
    }

    pub unsafe fn as_mut_void(&mut self) -> *mut *mut c_void {
        &mut self.0 as *mut *mut _ as *mut *mut _
    }
}

// Reference to managed pointers with an Interface/IUnknown base (supporting reference counting with AddRef/Release)
// need to be released manually using the destroy() function. Drop trait is not implemented.
impl<T: Interface> ComPtr<T> {
    pub unsafe fn as_unknown(&self) -> &IUnknown {
        debug_assert!(!self.is_null());
        &*(self.0 as *mut IUnknown)
    }

    // Cast creates a new ComPtr requiring explicit destroy call to avoid memory leaks.
    pub unsafe fn cast<U>(self) -> Result<ComPtr<U>, i32>
    where
        U: Interface,
    {
        let mut p = ComPtr::<U>::null();
        let hr = self
            .as_unknown()
            .QueryInterface(&U::uuidof(), p.as_mut_void());
        if FAILED(hr) {
            return Err(hr);
        }
        Ok(p)
    }

    // Destroying one instance of the ComPtr will invalidate all copies and clones.
    pub unsafe fn destroy(self) {
        self.as_unknown().Release();
    }
}

// Shallow clone as reference count is NOT increased.
impl<T> Clone for ComPtr<T> {
    fn clone(&self) -> Self {
        ComPtr(self.0)
    }
}

impl<T> Copy for ComPtr<T> {}

impl<T> Deref for ComPtr<T> {
    type Target = T;
    fn deref(&self) -> &T {
        debug_assert!(!self.is_null());
        unsafe { &*self.0 }
    }
}

impl<T> fmt::Debug for ComPtr<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ComPtr( ptr: {:?} )", self.0)
    }
}

impl<T> PartialEq<*mut T> for ComPtr<T> {
    fn eq(&self, other: &*mut T) -> bool {
        self.0 == *other
    }
}

impl<T> PartialEq for ComPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
