use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize};

struct OneShot<T> {
    message: UnsafeCell<MaybeUninit<T>>,
    ready: std::sync::atomic::AtomicBool,
}

pub struct Sender<'a, T> {
    chan: &'a OneShot<T>,
    receiving_thread: std::thread::Thread,
}

pub struct Receiver<'a, T> {
    chan: &'a OneShot<T>,
    _no_send: PhantomData<*const ()>, // !Send
}

impl<T> Sender<'_, T> {
    pub fn send(self, message: T) {
        unsafe { (*self.chan.message.get()).write(message) };
        self.chan
            .ready
            .store(true, std::sync::atomic::Ordering::Release);

        self.receiving_thread.unpark();
    }
}

impl<T> Receiver<'_, T> {
    pub fn receive(self) -> T {
        while self
            .chan
            .ready
            .compare_exchange(
                true,
                false,
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            std::thread::park();
        }

        unsafe { (*self.chan.message.get()).assume_init_read() }
    }
}

unsafe impl<T> Sync for OneShot<T> where T: Send {}

impl<T> OneShot<T> {
    pub const fn new() -> Self {
        Self {
            message: UnsafeCell::new(MaybeUninit::uninit()),
            ready: AtomicBool::new(false),
        }
    }

    pub fn spllit(&mut self) -> (Sender<T>, Receiver<T>) {
        *self = Self::new();
        (
            Sender {
                chan: self,
                receiving_thread: std::thread::current(),
            },
            Receiver {
                chan: self,
                _no_send: PhantomData,
            },
        )
    }
}

impl<T> Drop for OneShot<T> {
    fn drop(&mut self) {
        if *self.ready.get_mut() {
            unsafe { self.message.get_mut().assume_init_drop() }
        }
    }
}

struct Inner<T> {
    ref_count: AtomicUsize,
    data: T,
}

pub struct Arc<T> {
    ptr: std::ptr::NonNull<Inner<T>>,
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}

impl<T> Arc<T> {
    pub fn new(data: T) -> Self {
        Self {
            ptr: std::ptr::NonNull::from(Box::leak(Box::new(Inner {
                ref_count: AtomicUsize::new(0),
                data,
            }))),
        }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &self.ptr.as_ref().data }
    }
}

fn main() -> Result<(), std::io::Error> {
    let mut oneshot = OneShot::<i32>::new();
    let (sender, receiver) = oneshot.spllit();
    sender.send(1);
    Ok(())
}
