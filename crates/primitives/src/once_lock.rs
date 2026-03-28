//! `OnceLock` abstraction that uses [`std::sync::OnceLock`] when available, once_cell otherwise.

#[cfg(target_os = "solana")]
mod solana_impl {
    use core::cell::UnsafeCell;

    /// A Solana SBF-specific `OnceLock` that avoids atomic operations.
    ///
    /// Solana programs are single-threaded, so a non-atomic implementation is sufficient.
    pub struct OnceLock<T> {
        inner: UnsafeCell<Option<T>>,
    }

    unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}
    unsafe impl<T: Send> Send for OnceLock<T> {}

    impl<T> core::fmt::Debug for OnceLock<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("OnceLock(..)")
        }
    }

    impl<T> Default for OnceLock<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> OnceLock<T> {
        #[inline]
        pub const fn new() -> Self {
            Self {
                inner: UnsafeCell::new(None),
            }
        }

        #[inline]
        pub fn get_or_init<F>(&self, f: F) -> &T
        where
            F: FnOnce() -> T,
        {
            // SAFETY: Solana program execution is single-threaded.
            unsafe {
                let slot = &mut *self.inner.get();
                if slot.is_none() {
                    *slot = Some(f());
                }
                slot.as_ref().expect("OnceLock initialized")
            }
        }

        #[inline]
        pub fn get(&self) -> Option<&T> {
            // SAFETY: Immutable access to inner slot.
            unsafe { (&*self.inner.get()).as_ref() }
        }

        #[inline]
        pub fn set(&self, value: T) -> Result<(), T> {
            // SAFETY: Solana program execution is single-threaded.
            unsafe {
                let slot = &mut *self.inner.get();
                if slot.is_some() {
                    Err(value)
                } else {
                    *slot = Some(value);
                    Ok(())
                }
            }
        }
    }
}

#[cfg(not(feature = "std"))]
mod no_std_impl {
    use once_cell::race::OnceBox;
    use std::boxed::Box;

    /// A thread-safe cell which can be written to only once.
    #[derive(Debug)]
    pub struct OnceLock<T> {
        inner: OnceBox<T>,
    }

    impl<T> Default for OnceLock<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> OnceLock<T> {
        /// Creates a new empty OnceLock.
        #[inline]
        pub const fn new() -> Self {
            Self {
                inner: OnceBox::new(),
            }
        }

        /// Gets the contents of the OnceLock, initializing it if necessary.
        #[inline]
        pub fn get_or_init<F>(&self, f: F) -> &T
        where
            F: FnOnce() -> T,
        {
            self.inner.get_or_init(|| Box::new(f()))
        }

        /// Gets the contents of the OnceLock, returning None if it is not initialized.
        #[inline]
        pub fn get(&self) -> Option<&T> {
            self.inner.get()
        }

        /// Sets the contents of the OnceLock.
        #[inline]
        pub fn set(&self, value: T) -> Result<(), T> {
            self.inner.set(Box::new(value)).map_err(|e| *e)
        }
    }
}

#[cfg(all(feature = "std", not(target_os = "solana")))]
use once_cell as _;
#[cfg(all(feature = "std", not(target_os = "solana")))]
pub use std::sync::OnceLock;

#[cfg(all(not(feature = "std"), not(target_os = "solana")))]
pub use no_std_impl::OnceLock;

#[cfg(target_os = "solana")]
pub use solana_impl::OnceLock;
