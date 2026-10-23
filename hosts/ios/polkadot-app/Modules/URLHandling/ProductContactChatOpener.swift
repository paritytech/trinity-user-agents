import Foundation
import KeyDerivation
import Products
import SubstrateSdk
import TrUAPIHost

/// A chat product that takes what the user opened from an entry point of the host (`chat.openLink`).
protocol ProductChatLinkTaking: Sendable {
    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem
}

/// Opens the chat with an account in the chat product, where the native chat opens its chat with that account: a
/// contact picked in the "+" panel, a scanned address or a `chat?id=` link (HX2). The product answers with the room
/// of the contact, which it makes for an account that is no contact yet, and the room screen opens.
@MainActor
final class ProductContactChatOpener {
    static let shared = ProductContactChatOpener()

    /// The room that takes the contact links of the chat product.
    static let linkRoomId = "chats"

    private var registry: ChatExtensionsRegistering?
    private let tldProvider: DotNsTldProviding
    private let logger: LoggerProtocol

    init(tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared, logger: LoggerProtocol = Logger.shared) {
        self.tldProvider = tldProvider
        self.logger = logger
    }

    /// The registry that gives the bot of the chat product. The chat flow attaches it when it starts.
    func attach(registry: ChatExtensionsRegistering) {
        self.registry = registry
    }

    /// The bot of the chat product, when it runs.
    func chatProductBot(productId: String) -> ChatExtensionBotProtocol? {
        registry?.getChatExtensionBot(for: productId)
    }

    /// The payload of the contact link of an account: `contact:` and the account in lowercase hex.
    static func payload(account: AccountId) -> String {
        "contact:" + account.toHex()
    }

    /// Asks the chat product for the room of the account, and opens it. An account that the product cannot reach,
    /// for example one with no chat key, opens nothing, as in the native chat.
    func open(account: AccountId, navigator: ModuleNavigating) {
        guard let tld = tldProvider.currentTld() else {
            logger.warning("Contact chat not opened: no TLD")
            return
        }

        let productId = BuiltInProduct.chat(for: tld)
        guard let product = chatProductBot(productId: productId) as? ProductChatLinkTaking else {
            logger.warning("Contact chat not opened: the chat product does not run")
            return
        }

        Task { [logger] in
            do {
                let answer = try await product.openLink(ProductChatOpenLinkRequest(
                    roomId: Self.linkRoomId,
                    payload: Self.payload(account: account)
                ))
                guard let roomId = answer.roomId else {
                    logger.warning("Contact chat not opened: the chat product gave no room")
                    return
                }
                navigator.openChat(.chatExtension(productId, roomId: roomId))
            } catch {
                logger.warning("Contact chat not opened: \(error)")
            }
        }
    }
}

protocol BlockUserServicing {
    func blockUser(accountId: AccountId) async throws
    func unblockUser(accountId: AccountId) async throws
}

/// Blocks and unblocks a contact of the chat product from the screens outside its room, as the blocked users of the
/// settings do in the native chat. The room of the contact gets the action of the user (`Block`, `Unblock`).
final class ProductContactBlocking: BlockUserServicing, @unchecked Sendable {
    struct NoRoom: Error {}

    private let contacts: ProductContactSearch

    init(contacts: ProductContactSearch = ProductContactSearch()) {
        self.contacts = contacts
    }

    func blockUser(accountId: AccountId) async throws {
        try await perform(.block, accountId: accountId)
    }

    func unblockUser(accountId: AccountId) async throws {
        try await perform(.unblock, accountId: accountId)
    }

    private func perform(_ action: ChatUserAction, accountId: AccountId) async throws {
        guard
            let contact = try await contacts.contacts().first(where: { $0.accountId == accountId }),
            case let .chatExtension(productId, roomId?) = contact.chatId,
            let bot = await ProductContactChatOpener.shared.chatProductBot(productId: productId)
            as? ProductChatQuerying
        else {
            throw NoRoom()
        }

        _ = try await bot.userAction(ProductChatUserActionRequest(roomId: roomId, action: action))
    }
}
