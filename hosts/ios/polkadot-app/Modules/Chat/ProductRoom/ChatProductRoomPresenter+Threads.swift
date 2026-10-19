import Foundation
import PolkadotUI

/// The thread rooms of a product room (07-LATER §9.2, D32, scenario S64). A thread is a room of the product whose
/// features name a message of another room. The chat list does not show it: the chip under the root opens it, and
/// the back button returns to the parent room at the root.
extension ChatProductRoomPresenter {
    /// The message of another room that this room answers, or nil for a room that is not a thread.
    var threadParent: Chat.ThreadParent? {
        policy?.features.threadOf
    }

    /// The header of a thread room: "Thread", and the name of the parent room under it.
    func threadHeader() -> (title: String, subtitle: String?)? {
        guard let parent = threadParent else { return nil }
        let parentName = additions.threadRoot?.parentName ?? parent.roomId
        return (String(localized: .ChatExtension.threadTitle), parentName)
    }

    /// Loads the parent and the root of a thread room when the room declares or changes its parent.
    func loadThreadRootIfNeeded() {
        guard
            let parent = threadParent,
            parent != additions.threadParent,
            let resolver = additions.dependencies.threadRoots
        else {
            return
        }

        additions.threadParent = parent
        additions.tasks.append(Task { [weak self, chatId] in
            let root = await resolver.root(of: parent, threadChatId: chatId)
            guard let self, additions.threadParent == parent else { return }
            additions.threadRoot = root
            mark("thread", "Thread · \(root.parentName ?? parent.roomId)")
            mark("root", root.text ?? String(localized: .chatReplyPlaceholder))
            onChange?()
        })
    }

    /// Opens the thread of a message. The screen of the thread gets the root, and this screen shows the root again
    /// when the user comes back.
    func openThread(_ summary: Chat.ThreadSummary, rootMessageId: Chat.MessageId) {
        guard case let .chatExtension(productId, _) = chatId else { return }

        let threadChatId = Chat.Id.chatExtension(productId, roomId: summary.roomId)
        if let message = messages[rootMessageId] {
            additions.dependencies.threadRoots?.handOver(
                ChatThreadRoot(
                    parentName: appearanceFactory.productName,
                    author: rootAuthor(of: message),
                    text: message.content.threadRootText
                ),
                threadChatId: threadChatId
            )
        }

        additions.returnToMessageId = productMessageId(rootMessageId)
        mark("thread-open", summary.roomId)
        wireframe.showThread(threadChatId, from: view)
    }

    /// A thread room that opened without its parent room below it (from a link) puts the parent room below it, so
    /// the back button returns to the parent at the root. The step waits one turn of the main queue, because the
    /// navigation of a link removes the screens between the list and the new screen after the push.
    func placeParentBelowIfNeeded() {
        guard let parent = threadParent, case let .chatExtension(productId, _) = chatId else { return }

        let parentChatId = Chat.Id.chatExtension(productId, roomId: parent.roomId)
        DispatchQueue.main.async { [weak self] in
            guard let self, additions.isOnScreen else { return }
            wireframe.placeParentRoom(parentChatId, returnTo: parent.messageId, below: view)
        }
    }

    /// Shows the root of the thread that the user comes back from, when the window holds it.
    func returnToRootIfShown() {
        guard
            additions.isOnScreen,
            let productId = additions.returnToMessageId,
            let messageId = Chat.ProductMessageId.candidates(productId, in: chatId)
                .first(where: { messages[$0] != nil })
        else {
            return
        }

        additions.returnToMessageId = nil
        mark("returned", productId)
        DispatchQueue.main.async { [weak self] in
            self?.view?.scrollToMessage(messageId)
        }
    }

    /// The pinned quote at the top of a thread room: the root, or "Earlier message" when the host does not hold it.
    func threadRootConfiguration() -> ChatThreadRootView? {
        guard threadParent != nil, let root = additions.threadRoot else { return nil }

        return ChatThreadRootView(
            author: root.author ?? root.parentName ?? "",
            text: root.text ?? String(localized: .chatReplyPlaceholder)
        )
    }
}

private extension ChatProductRoomPresenter {
    func rootAuthor(of message: Chat.LocalMessage) -> String {
        if message.status.isOutgoing {
            return String(localized: .chatReplyYou)
        }

        if let participantId = message.authorParticipantId {
            return persona(of: participantId)?.name
                ?? participants.participant(participantId).map { appearanceFactory.appearance(of: $0).name }
                ?? participantId
        }

        return appearanceFactory.productName
    }
}
