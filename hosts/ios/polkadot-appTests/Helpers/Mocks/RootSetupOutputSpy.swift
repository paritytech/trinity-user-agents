import Foundation
@testable import polkadot_app

/// Records interactor output calls for testing Root setup behavior.
@MainActor
final class RootSetupOutputSpy: RootInteractorOutputProtocol {
    enum Event {
        case decided
        case failed(RootSetupFailureKind)
        case recoveredConnectivity
    }

    var didDecideCallCount = 0
    private(set) var failureKinds: [RootSetupFailureKind] = []
    private(set) var didRecoverConnectivityCallCount = 0

    private var eventBuffer: [Event] = []
    private var pendingContinuation: CheckedContinuation<Event?, Never>?

    init() {}

    var didFailSetupCallCount: Int { failureKinds.count }

    func didDecide(destination _: RootDestination) {
        didDecideCallCount += 1
        recordEvent(.decided)
    }

    func didFailSetup(kind: RootSetupFailureKind) {
        failureKinds.append(kind)
        recordEvent(.failed(kind))
    }

    func didRecoverConnectivity() {
        didRecoverConnectivityCallCount += 1
        recordEvent(.recoveredConnectivity)
    }

    func didRequireAppFactoryReset() {}

    /// Suspends until the next setup failure is reported and returns its kind.
    func nextFailureKind() async -> RootSetupFailureKind? {
        while let event = await nextEvent() {
            if case let .failed(kind) = event {
                return kind
            }
        }
        return nil
    }

    /// Suspends until the next destination decision is reported.
    func nextDecision() async {
        while let event = await nextEvent() {
            if case .decided = event {
                return
            }
        }
    }

    /// Suspends until the next connectivity recovery is reported.
    func nextRecovery() async {
        while let event = await nextEvent() {
            if case .recoveredConnectivity = event {
                return
            }
        }
    }

    private func recordEvent(_ event: Event) {
        if let continuation = pendingContinuation {
            pendingContinuation = nil
            continuation.resume(returning: event)
        } else {
            eventBuffer.append(event)
        }
    }

    private func nextEvent() async -> Event? {
        if !eventBuffer.isEmpty {
            return eventBuffer.removeFirst()
        }

        return await withCheckedContinuation { continuation in
            pendingContinuation = continuation
        }
    }
}
