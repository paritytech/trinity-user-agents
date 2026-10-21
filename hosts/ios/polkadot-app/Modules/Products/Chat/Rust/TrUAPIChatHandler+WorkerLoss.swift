import Foundation
import TrUAPIHost

/// The worker of the product stops under an open room only when it fails, because the open room holds it
/// (06-OPEN-ITEMS O26, scenario S23).
extension TrUAPIChatHandler {
    /// The execution of the worker that runs now. Throws `ChatSeamError.notStarted` when no worker runs.
    func requireExecution() throws -> TrUAPIProductExecutionProtocol {
        guard let execution = workers.currentExecution(of: productId) else {
            throw ChatSeamError.notStarted
        }
        return execution
    }

    /// Follows the worker of the product. The rooms on screen hold the worker, so the worker stops while a room
    /// is open only when it fails (06-OPEN-ITEMS O26). The open rooms then show "unavailable".
    func startFollowingWorker() {
        workerFollowingTask = Task { [weak self, workers, productId] in
            var runs = false
            do {
                for try await execution in workers.executions(of: productId) {
                    guard !Task.isCancelled else { return }
                    let nowRuns = execution != nil
                    guard nowRuns != runs else { continue }

                    runs = nowRuns
                    if !nowRuns {
                        await self?.workerStopped()
                    }
                }
            } catch {
                return
            }
        }
    }

    /// A product rings only while its worker runs, so the rooms of the product stop to ring (07-LATER §11.2).
    func workerStopped() async {
        calls.clearRinging(ofProduct: productId)
        guard !disposed, !openRooms.isEmpty else { return }

        logger.warning("[truapi] \(productId)'s worker stopped while rooms were open: \(openRooms.keys.sorted())")
        for roomId in openRooms.keys {
            await feedListeners.productStopped(for: .chatExtension(productId, roomId: roomId))
        }
    }
}
