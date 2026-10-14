@testable import BackgroundExecution

/// Records begin/end calls and lets a test fire the OS expiration handler on demand.
@MainActor
final class BackgroundTaskHostMock: BackgroundTaskHost {
    private(set) var beginCallCount = 0
    private(set) var endCallCount = 0

    private let token: BackgroundTaskToken
    private var expiration: (@Sendable () -> Void)?

    init(token: BackgroundTaskToken = BackgroundTaskToken(rawValue: 1)) {
        self.token = token
    }

    func beginBackgroundTask(expiration: @escaping @Sendable () -> Void) -> BackgroundTaskToken {
        beginCallCount += 1
        self.expiration = expiration
        return token
    }

    func endBackgroundTask(_: BackgroundTaskToken) {
        endCallCount += 1
    }

    func fireExpiration() {
        expiration?()
    }
}
