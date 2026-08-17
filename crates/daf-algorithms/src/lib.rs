#![allow(clippy::assertions_on_constants)]
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use daf_core::{AlgorithmError, AlgorithmStats};

#[derive(Debug, Default)]
struct FibonacciState {
    memo: HashMap<i64, i64>,
    iterations: u64,
    cache_hits: u64,
}

#[derive(Debug)]
pub struct FibonacciDP {
    state: tokio::sync::Mutex<FibonacciState>,
}

impl FibonacciDP {
    pub fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(FibonacciState::default()),
        }
    }

    fn compute_fib(state: &mut FibonacciState, n: i64) -> Result<i64, AlgorithmError> {
        debug_assert!(n >= 0, "fibonacci input must be non-negative");
        if let Some(&value) = state.memo.get(&n) {
            state.cache_hits += 1;
            return Ok(value);
        }

        if n <= 1 {
            state.iterations += 1;
            state.memo.insert(n, n);
            return Ok(n);
        }

        state.iterations += 1;
        let fib_n_minus_1 = Self::compute_fib(state, n - 1)?;
        let fib_n_minus_2 = Self::compute_fib(state, n - 2)?;
        let result = fib_n_minus_1 + fib_n_minus_2;

        state.memo.insert(n, result);
        Ok(result)
    }
}

impl Default for FibonacciDP {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl daf_core::Algorithm for FibonacciDP {
    async fn execute(
        &self,
        input: Arc<dyn Any + Send + Sync>,
    ) -> Result<Arc<dyn Any + Send + Sync>, AlgorithmError> {
        let n = *input
            .downcast_ref::<i64>()
            .ok_or_else(|| AlgorithmError::new("Expected i64 input for FibonacciDP"))?;
        debug_assert!(n >= 0, "fibonacci input must be non-negative, got {}", n);
        let mut state = self.state.lock().await;
        state.memo.clear();
        state.iterations = 0;
        state.cache_hits = 0;

        let result = Self::compute_fib(&mut state, n)?;
        debug_assert!(result >= 0, "fibonacci result must be non-negative");
        Ok(Arc::new(result))
    }

    async fn get_stats(&self) -> Result<AlgorithmStats, AlgorithmError> {
        let state = self.state.lock().await;
        let stats = AlgorithmStats::new(
            state.iterations,
            state.cache_hits,
            state.memo.len(),
        );
        debug_assert!(stats.iterations >= stats.cache_hits, "iterations must be >= cache_hits");
        Ok(stats)
    }
}