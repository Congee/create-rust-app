use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicBool, AtomicUsize};

struct Inner<T> {
    // + when Weak::clone(), Arc::clone() which is thru Weak::clone()
    // - when Weak::drop()
    pub alloc_rc: AtomicUsize,
    // + when Arc::clone(),
    // - when Arc::drop()
    pub data_rc: AtomicUsize,
    pub data: T,
}

pub struct Weak<T> {
    pub ptr: NonNull<Inner<T>>,
}

pub struct Arc<T> {
    inner: std::ptr::NonNull<Inner<T>>,
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}

impl<T> Arc<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: std::ptr::NonNull::from(Box::leak(Box::new(Inner {
                data_rc: AtomicUsize::new(0),
                data,
            }))),
        }
    }

    pub fn get_mut(arc: &mut Self) -> Option<&mut T> {
        unsafe {
            if arc
                .inner
                .as_ref()
                .data_rc
                .load(std::sync::atomic::Ordering::Relaxed)
                == 1
            {
                fence(std::sync::atomic::Ordering::Acquire);
                Some(&mut arc.inner.as_mut().data)
            } else {
                None
            }
        }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &self.inner.as_ref().data }
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        unsafe {
            if self
                .inner
                .as_ref()
                .data_rc
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                > usize::MAX / 2
            {
                std::process::abort();
            }
        }

        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        unsafe {
            if self
                .inner
                .as_ref()
                .data_rc
                .fetch_sub(1, std::sync::atomic::Ordering::Release)
                == 1
            {
                fence(std::sync::atomic::Ordering::Acquire);
                drop(Box::from_raw(self.inner.as_ptr()));
            }
        }
    }
}

#[test]
fn test() {
    fn test() {
        static  NUM_DROPS: AtomicUsize = AtomicUsize::new(0);
        struct DetectDrop;
        impl Drop for DetectDrop {
            fn drop(&mut self) {
                NUM_DROPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        let x = Arc::new(("hello", DetectDrop));
        let y = x.clone();

        let t = std::thread::spawn(move || {
            assert_eq!(x.0, "hello");
        });

        assert_eq!(y.0, "hello");

        t.join().unwrap();

        assert_eq!(NUM_DROPS.load(std::sync::atomic::Ordering::Relaxed), 0);

        drop(y);

        assert_eq!(NUM_DROPS.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}

fn main() -> Result<(), std::io::Error> {
    Ok(())
}
