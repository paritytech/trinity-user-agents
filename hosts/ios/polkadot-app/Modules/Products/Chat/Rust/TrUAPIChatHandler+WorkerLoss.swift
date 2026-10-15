import Foundation
import TrUAPIHost

/// The worker of the product stops under an open room only when it fails, because the open room holds it
/// (06-OPEN-ITEMS O26, scenario S23).
extension TrUAPIChatHandler {
    /// Follows the worker of the product. The rooms on screen hold the worker, so the worker stops while a room
    /// is open only when it fails (06-OPEN-ITEMS O26). The open rooms then show "unavailable", and they send
    /// `RoomOpened` to the next worker.
    func startFollowingWorker() {
        workerFollowingTask = Task { [weak self, workers, productId] in
            var runs = false
            do {
                for try await execution in workers.executions(of: productId) {
                    guard !Task.isCancelled else { return }
                    let nowRuns = execution != nil
                    guard nowRuns != runs else { continue }

                    runs = nowRuns
                    if nowRuns {
                        await self?.workerStarted()
                    } else {
                        await self?.workerStopped()
                    }
                }
            } catch {
                return
            }
        }
    }

    func workerStopped() async {
        guard !disposed, !openRooms.isEmpty else { return }

        roomsLostWorker = true
        logger.warning("[truapi] \(productId)'s worker stopped while rooms were open: \(openRooms.keys.sorted())")
        for roomId in openRooms.keys {
            let chatId = Chat.Id.chatExtension(productId, roomId: roomId)
            activityStore.set(nil, for: chatId)
            await feedListeners.productStopped(for: chatId)
        }
    }

    func workerStarted() {
        guard !disposed, roomsLostWorker else { return }

        roomsLostWorker = false
        for roomId in openRooms.keys.sorted() {
            publishRoomEvent(.roomOpened, roomId: roomId)
        }
    }

    /// The room screen does not wait for the product to take the event, and a lost event is logged.
    func publishRoomEvent(_ event: ChatEvent, roomId: String) {
        do {
            guard let execution = workers.currentExecution(of: productId) else {
                throw ChatSeamError.notStarted
            }
            try execution.publishChatEvent(roomId: roomId, peer: Self.hostUserPeer, event: event)
        } catch {
            logger.error("Rust chat runtime failed to send a room event to \(roomId): \(error)")
        }
    }
}
