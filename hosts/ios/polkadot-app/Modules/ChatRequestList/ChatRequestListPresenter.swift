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
            guard case let .person(contact) = chat.peer else {
                return nil
            }

            guard let chatRequest = contact.chatRequest else {
                return nil
            }

            guard
                chatRequest.isIncoming,
                case let .incoming(incomingStatus) = chatRequest.message?.status else {
                return nil
            }

            let contactName = contact.username
            let date = Date.fromChatTimestamp(chatRequest.timestamp)

            let messageText: String = chatRequest.requestListMessage()

            let avatarViewModel: AvatarViewModel =
                if let image = contact.imageData.flatMap({ UIImage(data: $0) }) {
                    .image(image)
                } else {
                    .colored(
                        text: String(contactName.prefix(1)),
                        colorSeed: chat.chatId.colorSeed
                    )
                }

            return ChatRequestListItem(
                id: chat.identifier,
                contactName: contactName,
                avatarViewModel: avatarViewModel,
                messageText: messageText,
                date: date,
                isSeen: incomingStatus == .seen
            )
        }
    }
}

private extension Chat.Request {
    func requestListMessage() -> String {
        switch message?.content {
        case let .chatRequest(content):
            content.welcomeMessage?.text ?? String(localized: .noMessage)
        case let .versionedChatRequest(content):
            content.ensureV1().welcomeMessage?.text ?? String(localized: .noMessage)
        default:
            String(localized: .noMessage)
        }
    }
}
