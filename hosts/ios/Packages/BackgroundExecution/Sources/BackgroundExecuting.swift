import Foundation

/// Runs an async operation while holding an OS background-task assertion, so the work can
/// continue during the short window iOS grants after the app is backgrounded (folded).
public protocol BackgroundExecuting: Sendable {
    /// Runs `operation` under a background-task assertion, relinquishing it once the
    /// operation finishes, throws, or the OS reclaims the window.
    ///
    /// - Throws: ``BackgroundExecutionExpiredError`` if the OS reclaimed the background
    ///   window before `operation` finished.
    ///   Otherwise rethrows the operation's own error.
    func execute<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T
}

/// Thrown by ``BackgroundExecuting/execute(_:)`` when the OS background window expired
/// before the operation completed, cancelling it.
public struct BackgroundExecutionExpiredError: Error, Sendable, Equatable {
    public init() {}
}
