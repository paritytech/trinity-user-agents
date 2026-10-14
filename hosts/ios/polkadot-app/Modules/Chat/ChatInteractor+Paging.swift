import Foundation

/// The paging, jump and room lifecycle parts of the room screen (P5.3, P5.7).
extension ChatInteractor {
    func loadOlderMessages() {
        Task { [engine, state] in
            guard await state.paging.older.canAsk else {
                return
            }

            _ = await engine.loadOlder()
        }
    }

    func loadNewerMessages() {
        Task { [engine, state] in
            guard await state.paging.newer.canAsk else {
                return
            }

            _ = await engine.loadNewer()
        }
    }

    func openRoom() {
        enqueueRoomWork { await $0.openRoom() }
    }

    func closeRoom() {
        enqueueRoomWork { await $0.closeRoom() }
    }

    func retryRoom() {
        enqueueRoomWork { await $0.retryRoom() }
    }

    func jump(to messageId: Chat.MessageId) {
        Task { [weak self, engine] in
            let found = await engine.jump(to: messageId)
            await self?.presenter?.didJump(to: messageId, found: found)
        }
    }
}

private extension ChatInteractor {
    /// Runs the lifecycle work of the room in the order of the calls, so a close never passes an open.
    func enqueueRoomWork(_ work: @escaping @Sendable (ChatEngineProtocol) async -> Void) {
        let previous = roomTask
        roomTask = Task { [engine] in
            await previous?.value
            await work(engine)
        }
    }
}

private extension ChatPagingState.Edge {
    /// True when a page for this end can come: more messages exist, or the last page failed.
    var canAsk: Bool {
        self == .more || self == .failed
    }
}
