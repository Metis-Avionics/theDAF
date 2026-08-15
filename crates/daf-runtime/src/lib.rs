use tokio::runtime::Runtime as TokioRuntime;

#[derive(Debug, Clone)]
pub struct Handle {
    inner: tokio::runtime::Handle,
}

impl Handle {
    pub fn current() -> Self {
        Self {
            inner: tokio::runtime::Handle::current(),
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future);
    }
}

#[derive(Debug)]
pub struct Runtime {
    inner: TokioRuntime,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            inner: TokioRuntime::new().expect("failed to create tokio runtime"),
        }
    }

    pub fn handle(&self) -> Handle {
        Handle {
            inner: self.inner.handle().clone(),
        }
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.inner.block_on(future)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
