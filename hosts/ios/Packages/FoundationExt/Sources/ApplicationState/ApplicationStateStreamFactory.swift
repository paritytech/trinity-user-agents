import UIKit

/// Actor-friendly application lifecycle observer that exposes events as `AsyncStream`.
/// Replaces delegate-based `ApplicationHandler` for use in Swift concurrency contexts.
///
/// Each call to ``stream(for:)`` returns an independent stream, supporting multiple consumers.
public final class ApplicationStateStreamFactory: @unchecked Sendable {
    public enum Event {
        case willResignActive
        case didBecomeActive
        case willEnterForeground
        case didEnterBackground
    }

    public init() {}

    /// Creates a new `AsyncStream<Void>` that yields each time the specified event occurs.
    public func stream(for event: Event) -> AsyncStream<Void> {
        AsyncStream { continuation in
            let observer = NotificationCenter.default.addObserver(
                forName: event.notificationName,
                object: nil,
                queue: nil
            ) { _ in
                continuation.yield()
            }

            continuation.onTermination = { _ in
                NotificationCenter.default.removeObserver(observer)
            }
        }
    }
}

// MARK: - Event → Notification.Name

private extension ApplicationStateStreamFactory.Event {
    var notificationName: Notification.Name {
        switch self {
        case .willResignActive: UIApplication.willResignActiveNotification
        case .didBecomeActive: UIApplication.didBecomeActiveNotification
        case .willEnterForeground: UIApplication.willEnterForegroundNotification
        case .didEnterBackground: UIApplication.didEnterBackgroundNotification
        }
    }
}
