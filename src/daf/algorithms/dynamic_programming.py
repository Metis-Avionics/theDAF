"""Dynamic programming algorithms with explicit memoization."""

from typing import Any


class FibonacciDP:
    """Fibonacci algorithm using dynamic programming with explicit memoization.
    
    Demonstrates recursive subproblem decomposition with memoized reuse.
    Tracks iterations and cache hits to show the efficiency benefit.
    """

    def __init__(self) -> None:
        """Initialize the algorithm with empty memoization cache."""
        self._memo: dict[int, int] = {}
        self._iterations: int = 0
        self._cache_hits: int = 0

    async def execute(self, input_data: Any) -> int:
        """Execute the Fibonacci algorithm.
        
        Args:
            input_data: Expected to be an integer N for fib(N).
            
        Returns:
            The Nth Fibonacci number.
            
        Raises:
            ValueError: If input is not a valid positive integer.
        """
        if not isinstance(input_data, int) or input_data < 0:
            raise ValueError(f"Expected non-negative integer, got {input_data}")

        # Reset counters for this execution
        self._iterations = 0
        self._cache_hits = 0
        self._memo.clear()

        return await self._compute_fib(input_data)

    async def _compute_fib(self, n: int) -> int:
        """Compute Fibonacci number with memoization.
        
        Args:
            n: The Fibonacci index.
            
        Returns:
            The Nth Fibonacci number.
        """
        # Check memo first
        if n in self._memo:
            self._cache_hits += 1
            return self._memo[n]

        # Count iteration
        self._iterations += 1

        # Base cases
        if n <= 1:
            result = n
        else:
            # Recursive case: decompose problem
            fib_n_minus_1 = await self._compute_fib(n - 1)
            fib_n_minus_2 = await self._compute_fib(n - 2)
            result = fib_n_minus_1 + fib_n_minus_2

        # Memoize result
        self._memo[n] = result
        return result

    async def get_stats(self) -> dict[str, Any]:
        """Get execution statistics.
        
        Returns:
            Dictionary with iteration count and cache hits.
        """
        return {
            "iterations": self._iterations,
            "cache_hits": self._cache_hits,
            "memo_size": len(self._memo),
        }
