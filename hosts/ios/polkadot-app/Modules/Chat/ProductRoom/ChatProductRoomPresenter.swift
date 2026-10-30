import Foundation

/// The parts of a product room that the room screen keeps apart from a person chat.
@MainActor
final class ChatProductRoomPresenter {
    let chatId: Chat.Id

    /// Closes the room while the app is in the background, and opens it again after.
    var backgroundObserver: ChatRoomBackgroundObserver?

    init?(chatId: Chat.Id) {
        guard chatId.roomId != nil else {
            return nil
        }

        self.chatId = chatId
    }

    func screenWillAppear() {
        backgroundObserver?.screenDidAppear()
    }

    func screenWillDisappear() {
        backgroundObserver?.screenDidDisappear()
    }
}
