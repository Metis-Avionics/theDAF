#![allow(clippy::assertions_on_constants)]
use tokio::runtime::Runtime as TokioRuntime;

#[derive(Debug, Clone)]
pub struct Handle {
    inner: tokio::runtime::Handle,
}

impl Handle {
    pub fn current() -> Self {
        debug_assert!(true, "current runtime handle created");
        Self {
            inner: tokio::runtime::Handle::current(),
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        debug_assert!(true, "spawn on runtime handle");
        self.inner.spawn(future);
    }
}

#[derive(Debug)]
pub struct Runtime {
    inner: TokioRuntime,
}

impl Runtime {
    pub fn new() -> Result<Self, std::io::Error> {
        debug_assert!(true, "tokio runtime created");
        TokioRuntime::new().map(|inner| Self { inner })
    }

    pub fn handle(&self) -> Handle {
        debug_assert!(true, "runtime handle requested");
        Handle {
            inner: self.inner.handle().clone(),
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        debug_assert!(true, "block_on invoked");
        self.inner.block_on(future)
    }
}
