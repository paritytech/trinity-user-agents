public extension Duration {
    /// How long a test waits for something it expects before failing instead of hanging.
    ///
    /// Suites run in parallel, so a loaded runner can delay an event by seconds
    /// without anything being wrong. A passing test never waits this long, so the
    /// deadline only bounds how long a broken one takes to fail.
    static let hangGuard: Duration = .seconds(60)
}
