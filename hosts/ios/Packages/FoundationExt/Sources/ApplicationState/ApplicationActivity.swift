import UIKit

@MainActor
public protocol ApplicationActivityProviding: Sendable {
    var isInBackground: Bool { get }

    /// Cancelling the caller ends the wait.
    func waitUntilInForeground() async
}

@MainActor
public final class ApplicationActivity {
    private let streamFactory: ApplicationStateStreamFactory

    public nonisolated init(streamFactory: ApplicationStateStreamFactory = ApplicationStateStreamFactory()) {
        self.streamFactory = streamFactory
    }
}

extension ApplicationActivity: ApplicationActivityProviding {
    public var isInBackground: Bool {
        UIApplication.shared.applicationState == .background
    }

    public func waitUntilInForeground() async {
        // Subscribed before the check, so a transition between the two can't be missed.
        let foregroundEvents = streamFactory.stream(for: .willEnterForeground)

        guard isInBackground else { return }

        for await _ in foregroundEvents {
            return
        }
    }
}
