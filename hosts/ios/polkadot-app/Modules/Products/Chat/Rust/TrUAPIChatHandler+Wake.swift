import Foundation
import StructuredConcurrency
import TrUAPIHost

/// The wake of a message in flight (W1 of 07-LATER §10.2).
///
/// A user message, a user action or an event of a room wakes the worker of the product. The core then holds the
/// worker for 30 seconds, so the product can answer when the user closes the room at once or the app goes to the
/// background. Each message wakes, also when a room screen holds the worker: the room screen can close before the
/// product answers. A new wake moves the end of the hold.
extension TrUAPIChatHandler {
    /// Wakes the worker for a message of `roomId`.
    func wakeForMessage(in roomId: String) async {
        await workers.wake(productId, reason: .messageInFlight(roomId: roomId))
    }

    /// Wakes the worker for a message of `roomId`, and gives its execution. A worker that does not run yet gets
    /// ``messageStartupWindow`` to start. Throws `ChatSeamError.notStarted` when no worker runs at the end.
    func executionForMessage(in roomId: String) async throws -> TrUAPIProductExecutionProtocol {
        let woken = await workers.wake(productId, reason: .messageInFlight(roomId: roomId)) != nil
        guard !disposed else { throw CancellationError() }

        if let execution = workers.currentExecution(of: productId) {
            return execution
        }
        guard woken else { throw ChatSeamError.notStarted }

        do {
            return try await withTimeout(messageStartupWindow) { [workers, productId] in
                for try await execution in workers.executions(of: productId) {
                    if let execution { return execution }
                }
                throw ChatSeamError.notStarted
            }
        } catch let error where !(error is CancellationError) {
            throw ChatSeamError.notStarted
        }
    }
}
