import Foundation

/// The room lifecycle parts of the room screen.
extension ChatInteractor {
    func openRoom() {
        enqueueRoomWork { await $0.openRoom() }
    }

    func closeRoom() {
        enqueueRoomWork { await $0.closeRoom() }
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
