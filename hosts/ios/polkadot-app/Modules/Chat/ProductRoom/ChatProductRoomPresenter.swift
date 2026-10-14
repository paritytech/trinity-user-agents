import Foundation
import TrUAPIHost
import UIKit

/// The parts of a product room that its product declares (02-DESIGN §3.3.7, §3.3.8, §8.6): the message menu, the
/// toolbar, the participants, who reacted, the footer and its extras.
///
/// The chat presenter keeps one for each product room. The view model factory reads it while it draws the room, so
/// the chat presenter needs only a few hooks. Each question to the product has the lookup time of 02-DESIGN §3.6.
/// When the product refuses a question or does not answer it in time, the room uses the host default for the item.
@MainActor
final class ChatProductRoomPresenter {
    weak var view: ChatProductRoomViewProtocol?

    /// Asks the room screen to draw the room again, for example when the product answered a question.
    var onChange: (() -> Void)?

    let chatId: Chat.Id
    let roomId: String

    let room: ProductRoomInteracting
    let wireframe: ChatProductRoomWireframing
    let resolver: ChatResourceResolving
    let logger: LoggerProtocol

    private(set) var policy: ChatRoomPolicy?
    /// The participants that the room screen knows, shared by the header, the bubbles and the list.
    let participants: ChatParticipantDirectory
    private(set) var appearanceFactory: ChatParticipantAppearanceFactory
    /// The messages of the window that the room screen shows, by id.
    private(set) var messages: [Chat.MessageId: Chat.LocalMessage] = [:]
    private var icons: [ChatResource: UIImage] = [:]
    /// The product answered `NotAvailable` to `reactions_query`, so the room hides who reacted.
    var reactionDetailsUnavailable = false

    init?(
        chatId: Chat.Id,
        room: ProductRoomInteracting,
        wireframe: ChatProductRoomWireframing,
        resolver: ChatResourceResolving = ChatResourceResolver.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        guard case let .chatExtension(productId, roomId?) = chatId else {
            return nil
        }

        self.chatId = chatId
        self.roomId = roomId
        self.room = room
        self.wireframe = wireframe
        self.resolver = resolver
        self.logger = logger

        participants = ChatParticipantDirectory(roomId: roomId, room: room, logger: logger)
        appearanceFactory = ChatParticipantAppearanceFactory(
            myUsername: "",
            productName: productId,
            productId: productId
        )
        participants.onChange = { [weak self] in self?.onChange?() }
    }

    /// The view model factory gives the room that it draws. The call never asks to draw again.
    func update(listModel: MessageListModel, metadata: MessageListMetadata) {
        appearanceFactory = ChatParticipantAppearanceFactory(
            myUsername: metadata.myUsername,
            productName: metadata.peerName,
            productId: appearanceFactory.productId
        )
        update(policy: metadata.chatMetadata.roomPolicy, messages: listModel.allMessages)
    }

    func update(policy: ChatRoomPolicy?, messages: [Chat.LocalMessage]) {
        let previous = self.policy
        self.policy = policy
        self.messages = Dictionary(messages.map { ($0.messageId, $0) }, uniquingKeysWith: { _, newest in newest })

        let participantsChanged = previous?.productQueries.contains(.participants)
            != policy?.productQueries.contains(.participants)

        if previous?.toolbarActions != policy?.toolbarActions || previous == nil || participantsChanged {
            provideToolbar()
        }

        loadParticipantsIfDeclared()
    }

    /// The id under which the product knows a message of the room.
    func productMessageId(_ messageId: Chat.MessageId) -> String {
        Chat.ProductMessageId.product(messageId, in: chatId)
    }
}

// MARK: - Product actions

extension ChatProductRoomPresenter {
    /// Runs a product entry of a menu or of the toolbar. A link opens after the user confirms it. A callback sends
    /// `event` to the product.
    func perform(_ handler: Chat.ProductActionHandler, event: ChatEvent) {
        switch handler {
        case let .deeplink(link):
            guard let url = URL(string: link) else {
                logger.warning("Product action of room \(roomId) has a link that is not a URL")
                return
            }

            wireframe.confirmProductLink(url, from: view) { [weak self] in
                self?.wireframe.openProductLink(url, from: self?.view)
            }
        case .callback:
            publish(event)
        }
    }

    /// Sends an event of the user to the product. A lost event is logged.
    func publish(_ event: ChatEvent) {
        Task { [room, roomId, logger] in
            do {
                try await room.publish(event, roomId: roomId)
            } catch {
                logger.error("Product room \(roomId) did not take an event: \(error)")
            }
        }
    }

    /// The images of the icons of product entries, by action id. An icon that does not load has no image.
    func icons(of actions: [Chat.ProductMenuAction]) async -> [String: UIImage] {
        var images: [String: UIImage] = [:]

        for action in actions {
            guard let resource = action.icon?.chatResource else {
                continue
            }

            if let cached = icons[resource] {
                images[action.actionId] = cached
            } else if let image = await resolver.image(for: resource) {
                icons[resource] = image
                images[action.actionId] = image
            }
        }

        return images
    }
}
