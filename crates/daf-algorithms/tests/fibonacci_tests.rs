use std::sync::Arc;

use daf_algorithms::FibonacciDP;
use daf_core::Algorithm;

#[tokio::test]
async fn fib_zero() {
    let algo = FibonacciDP::new();
    let result = algo.execute(Arc::new(0_i64)).await.unwrap();
    let n = result.downcast_ref::<i64>().unwrap();
    assert_eq!(*n, 0);
}

#[tokio::test]
async fn fib_one() {
    let algo = FibonacciDP::new();
    let result = algo.execute(Arc::new(1_i64)).await.unwrap();
    let n = result.downcast_ref::<i64>().unwrap();
    assert_eq!(*n, 1);
}

#[tokio::test]
async fn fib_five() {
    let algo = FibonacciDP::new();
    let result = algo.execute(Arc::new(5_i64)).await.unwrap();
    let n = result.downcast_ref::<i64>().unwrap();
    assert_eq!(*n, 5);
}

#[tokio::test]
async fn fib_ten() {
    let algo = FibonacciDP::new();
    let result = algo.execute(Arc::new(10_i64)).await.unwrap();
    let n = result.downcast_ref::<i64>().unwrap();
    assert_eq!(*n, 55);
}

#[tokio::test]
async fn fib_returns_stats() {
    let algo = FibonacciDP::new();
    algo.execute(Arc::new(10_i64)).await.unwrap();
    let stats = algo.get_stats().await.unwrap();
    assert!(stats.iterations < 100);
    assert!(stats.cache_hits > 0);
    assert_eq!(stats.memo_size, 11);
}

#[tokio::test]
async fn fib_clears_between_executions() {
    let algo = FibonacciDP::new();
    algo.execute(Arc::new(10_i64)).await.unwrap();
    let _s1 = algo.get_stats().await.unwrap();
    algo.execute(Arc::new(5_i64)).await.unwrap();
    let s2 = algo.get_stats().await.unwrap();
    assert!(s2.memo_size <= 6);
}

#[tokio::test]
async fn fib_accepts_arc_i64_input() {
    let algo = FibonacciDP::new();
    let input = Arc::new(7_i64);
    let result = algo.execute(input).await.unwrap();
    let n = result.downcast_ref::<i64>().unwrap();
    assert_eq!(*n, 13);
}

#[tokio::test]
async fn fib_get_stats_after_multiple_executes() {
    let algo = FibonacciDP::new();
    algo.execute(Arc::new(5_i64)).await.unwrap();
    algo.execute(Arc::new(10_i64)).await.unwrap();
    let stats = algo.get_stats().await.unwrap();
    assert!(stats.iterations > 0);
    assert!(stats.memo_size > 0);
}
