import UIKit

@MainActor
public protocol BackgroundTaskHost: Sendable {
    /// Requests background execution time, returning a handle to relinquish it later.
    /// - Parameter expiration: Invoked by the system when the granted time is about to
    ///   expire.
    func beginBackgroundTask(expiration: @escaping @Sendable () -> Void) -> BackgroundTaskToken

    /// Relinquishes previously requested background time. A no-op for an invalid token.
    func endBackgroundTask(_ token: BackgroundTaskToken)
}

extension UIApplication: BackgroundTaskHost {
    private static let backgroundTaskName = "BackgroundExecution.LongrunTask"

    public func beginBackgroundTask(
        expiration: @escaping @Sendable () -> Void
    ) -> BackgroundTaskToken {
        let identifier = beginBackgroundTask(
            withName: Self.backgroundTaskName,
            expirationHandler: expiration
        )

        return BackgroundTaskToken(identifier)
    }

    public func endBackgroundTask(_ token: BackgroundTaskToken) {
        endBackgroundTask(token.identifier)
    }
}

public struct BackgroundTaskToken: Sendable, Equatable {
    let rawValue: Int

    public static let invalid = BackgroundTaskToken(rawValue: UIBackgroundTaskIdentifier.invalid.rawValue)

    init(rawValue: Int) {
        self.rawValue = rawValue
    }

    init(_ identifier: UIBackgroundTaskIdentifier) {
        rawValue = identifier.rawValue
    }

    var identifier: UIBackgroundTaskIdentifier {
        UIBackgroundTaskIdentifier(rawValue: rawValue)
    }

    var isValid: Bool {
        self != .invalid
    }
}
