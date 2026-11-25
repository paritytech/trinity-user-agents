import Foundation
import FoundationExt
import StructuredConcurrencyTestSupport

@MainActor
final class ApplicationActivityStub: ApplicationActivityProviding {
    private(set) var isInBackground: Bool
    private(set) var waitCallCount = 0
    private(set) var cancelledWaitCount = 0

    let changes = StateChangeSignal()

    private let gate = TestGate()

    init(isInBackground: Bool = false) {
        self.isInBackground = isInBackground
    }

    func waitUntilInForeground() async {
        guard isInBackground else { return }

        waitCallCount += 1
        changes.signal()

        do {
            try await gate.pass()
        } catch {
            cancelledWaitCount += 1
            changes.signal()
        }
    }

    func enterForeground() {
        isInBackground = false

        gate.open()
        gate.release()
    }
}
