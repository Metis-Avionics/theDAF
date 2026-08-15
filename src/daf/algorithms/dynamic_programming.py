"""Dynamic programming algorithms with explicit memoization."""

from typing import Any

from daf.utils._memoize import Memo


class FibonacciDP:
    """Fibonacci algorithm using dynamic programming with explicit memoization.
    
    Demonstrates recursive subproblem decomposition with memoized reuse.
    Tracks iterations and cache hits to show the efficiency benefit.
    """

    def __init__(self) -> None:
        """Initialize the algorithm with empty memoization cache."""
        self._memo: Memo | None = None

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

        self._memo = Memo()
        return await self._compute_fib(input_data)

    async def _compute_fib(self, n: int) -> int:
        """Compute Fibonacci number with memoization.
        
        Args:
            n: The Fibonacci index.
            
        Returns:
            The Nth Fibonacci number.
        """
        memo = self._memo
        if memo is None:
            raise RuntimeError("execute() must be called before _compute_fib()")

        if memo.has(n):
            return memo.get(n)  # type: ignore[no-any-return]

        if n <= 1:
            return n

        fib_n_minus_1 = await self._compute_fib(n - 1)
        fib_n_minus_2 = await self._compute_fib(n - 2)
        result = fib_n_minus_1 + fib_n_minus_2

        memo.set(n, result)
        return result

    async def get_stats(self) -> dict[str, Any]:
        """Get execution statistics.
        
        Returns:
            Dictionary with iteration count and cache hits.
        """
        if self._memo is not None:
            return self._memo.stats()
        return {"iterations": 0, "cache_hits": 0, "memo_size": 0}
