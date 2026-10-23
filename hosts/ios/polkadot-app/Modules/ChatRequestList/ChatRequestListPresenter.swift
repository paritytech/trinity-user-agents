import Foundation
import PolkadotUI
import UIKit.UIImage

@MainActor
final class ChatRequestListPresenter {
    weak var view: ChatRequestListViewProtocol?
    let wireframe: ChatRequestListWireframeProtocol
    let interactor: ChatRequestListInteractorInputProtocol
    let dateFormatter: TimestampFormatting

    private var chats: [Chat.LocalModel]?

    init(
        interactor: ChatRequestListInteractorInputProtocol,
        wireframe: ChatRequestListWireframeProtocol,
        dateFormatter: TimestampFormatting = ContactTimestampFormatter()
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.dateFormatter = dateFormatter
    }
}

extension ChatRequestListPresenter: ChatRequestListPresenterProtocol {
    func setup() {
        interactor.setup()
    }
}

extension ChatRequestListPresenter: ChatRequestListInteractorOutputProtocol {
    func didReceiveChats(_ chats: [Chat.LocalModel]) {
        self.chats = chats

        let items = makeItems(from: chats)

        if items.isEmpty {
            wireframe.close(from: view)
        } else {
            view?.didReceive(items: items)
        }
    }
}

extension ChatRequestListPresenter {
    func selectRequest(with id: String) {
        guard let chats, let chat = chats.first(where: { $0.identifier == id }) else {
            return
        }

        wireframe.showChat(
            from: view,
            openModel: .existingChat(chat.chatId)
        )
    }
}

private extension ChatRequestListPresenter {
    func makeItems(from chats: [Chat.LocalModel]) -> [ChatRequestListItem] {
        chats.compactMap { chat in
            chat.roomContact.flatMap { makeItem(from: chat, contact: $0) }
        }
    }
}

private extension ChatRequestListPresenter {
    /// The request of a contact of the chat product, as the native chat lists the request of a contact.
    func makeItem(from chat: Chat.LocalModel, contact: Chat.RoomContact) -> ChatRequestListItem? {
        guard contact.state == .incoming else {
            return nil
        }

        let contactName = contact.name(fallback: chat.roomMetadata?.name ?? "")
        let timestamp = chat.message?.timestamp ?? chat.roomDeclarations?.preview.timestamp.map(UInt64.init) ?? 0

        return ChatRequestListItem(
            id: chat.identifier,
            contactName: contactName,
            avatarViewModel: .colored(text: String(contactName.prefix(1)), colorSeed: chat.avatarColorSeed),
            messageText: chat.message?.content.requestListMessage() ?? String(localized: .noMessage),
            date: Date.fromChatTimestamp(timestamp),
            isSeen: chat.message?.status != .incoming(.new)
        )
    }
}

private extension Chat.Request {
    func requestListMessage() -> String {
        message?.content.requestListMessage() ?? String(localized: .noMessage)
    }
}

private extension Chat.LocalMessage.Content {
    func requestListMessage() -> String {
        switch self {
        case let .chatRequest(content):
            content.welcomeMessage?.text ?? String(localized: .noMessage)
        case let .versionedChatRequest(content):
            content.ensureV1().welcomeMessage?.text ?? String(localized: .noMessage)
        default:
            String(localized: .noMessage)
        }
    }
}
