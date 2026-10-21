import Foundation

/// The chat-list row of a product room that rings shows "Incoming call" (07-LATER §11.2, scenario S73).
extension ContactsListPresenter {
    /// Draws the list again at each change of the call of a room.
    func followRoomCalls(_ store: ChatRoomCallStoring = ChatRoomCallStore.shared) -> Task<Void, Never> {
        Task { @MainActor [weak self] in
            for await change in store.changes() {
                guard let self else { return }
                if let lastModel {
                    provideViewModel(lastModel)
                }
                markCallPreview(change)
            }
        }
    }

    /// Writes `[e2e:ui] list-call <room> <preview>` for the harness.
    private func markCallPreview(_ change: Chat.RoomCallChange) {
        #if targetEnvironment(simulator)
            guard TrUAPIE2EMarkers.isEnabled, let roomId = change.chatId.roomId else { return }
            let preview = ChatRoomCallViewModel.listPreview(of: change.call) ?? "none"
            logger.debug("[e2e:ui] list-call \(roomId) \(preview)")
        #endif
    }
}
