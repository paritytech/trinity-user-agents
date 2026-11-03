import os
@testable import BackgroundExecution

final class BackgroundTaskSchedulerSpy: BackgroundTaskScheduling, @unchecked Sendable {
    private struct State {
        var registeredIdentifiers: [String] = []
        var launchHandler: (@Sendable (any BackgroundTaskHandle) -> Void)?
        var submittedRequests: [BackgroundTaskRequest] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let acceptsRegistration: Bool
    private let submitError: Error?

    init(acceptsRegistration: Bool = true, submitError: Error? = nil) {
        self.acceptsRegistration = acceptsRegistration
        self.submitError = submitError
    }

    var registeredIdentifiers: [String] {
        state.withLock(\.registeredIdentifiers)
    }

    var submittedRequests: [BackgroundTaskRequest] {
        state.withLock(\.submittedRequests)
    }

    func register(
        identifier: String,
        launchHandler: @escaping @Sendable (any BackgroundTaskHandle) -> Void
    ) -> Bool {
        state.withLock { state in
            state.registeredIdentifiers.append(identifier)
            state.launchHandler = launchHandler
        }
        return acceptsRegistration
    }

    func submit(_ request: BackgroundTaskRequest) throws {
        if let submitError {
            throw submitError
        }

        state.withLock { $0.submittedRequests.append(request) }
    }

    func pendingRequestIdentifiers() async -> [String] {
        submittedRequests.map(\.identifier)
    }

    func launch(_ task: any BackgroundTaskHandle) {
        let handler = state.withLock(\.launchHandler)
        handler?(task)
    }
}
