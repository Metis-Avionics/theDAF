#![allow(clippy::assertions_on_constants)]
use std::sync::OnceLock;
use tokio::runtime::Runtime as TokioRuntime;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Handle {
    inner: tokio::runtime::Handle,
}

impl Handle {
    pub fn current() -> Self {
        debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
        Self {
            inner: tokio::runtime::Handle::current(),
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
        self.inner.spawn(future);
    }
}

#[derive(Debug)]
pub struct Runtime {
    inner: TokioRuntime,
}

impl Runtime {
    pub fn new() -> Result<Self, std::io::Error> {
        TokioRuntime::new().map(|inner| Self { inner })
    }

    pub fn handle(&self) -> Handle {
        debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
        Handle {
            inner: self.inner.handle().clone(),
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        debug_assert!(RUNTIME.get().is_some(), "tokio runtime not initialised");
        self.inner.block_on(future)
    }
}