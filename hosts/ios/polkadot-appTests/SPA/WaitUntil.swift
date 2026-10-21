/// Yields until the condition holds. Bounded, so an unmet condition fails the expectation instead of hanging.
@MainActor
func waitUntil(_ condition: () -> Bool) async {
    for _ in 0 ..< 1_000 where !condition() {
        await Task.yield()
    }
}
