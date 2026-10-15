import Foundation
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The worker hold of a stage (PARITY row 49, D36). On a cold start, the core reads the declaration of the room only
/// through the open execution, so the host waits until the worker runs before it stages.
struct ProductStagingWorkerWaitTests {
    private let references = StubWorkerReferences()
    private let workers = StubWorkerManager()

    @Test func theHoldReturnsWhenTheWorkerRuns() async {
        let staging = makeStaging(executionWait: .seconds(10))
        let returned = Returned()

        let hold = Task {
            await staging.holdWorker(productId: "game.dot")
            returned.set()
        }
        try? await Task.sleep(for: .milliseconds(50))
        #expect(!returned.value)
        #expect(references.acquired == ["game.dot"])

        workers.publish(["game.dot": MockProductExecution()])
        await hold.value

        #expect(returned.value)
    }

    @Test func aWorkerThatRunsAlreadyDoesNotWait() async {
        workers.publish(["game.dot": MockProductExecution()])
        let staging = makeStaging(executionWait: .seconds(10))
        let start = ContinuousClock.now

        await staging.holdWorker(productId: "game.dot")

        #expect(ContinuousClock.now - start < .seconds(5))
        #expect(references.acquired == ["game.dot"])
    }

    /// The stage still runs after the bound. The core answers it, for example with `NotDeclared`.
    @Test func theHoldEndsAfterTheBoundWhenNoWorkerRuns() async {
        let staging = makeStaging(executionWait: .milliseconds(50))

        await staging.holdWorker(productId: "game.dot")
        staging.releaseWorker(productId: "game.dot")

        #expect(references.acquired == ["game.dot"])
        #expect(references.released == ["game.dot"])
    }

    private func makeStaging(executionWait: Duration) -> TrUAPIProductAttachmentStaging {
        TrUAPIProductAttachmentStaging(
            runtime: { throw ProductStageFailure.submitFailed(reason: "no runtime in the tests") },
            references: { [references] in references },
            workers: { [workers] in workers },
            executionWait: executionWait,
            logger: StubLogger()
        )
    }
}

private final class Returned: @unchecked Sendable {
    private let lock = NSLock()
    private var returned = false

    var value: Bool {
        lock.withLock { returned }
    }

    func set() {
        lock.withLock { returned = true }
    }
}
