import Foundation
import Operation_iOS
import Products
import TrUAPIHost

/// A link into the room of a product: `polkadot://<product_id>.<tld>/-/chat/<room_id>` (02-DESIGN §13 item 15), or
/// its web form `https://<name>.<root>.li/-/chat/<room_id>`. The text after `#` is a payload for the product, which
/// the host gives it with `chat.openLink` and does not read.
///
/// The core classifies the link as a dotNS page, because it does not serve the `chat` target of the reserved `-`
/// segment. So the host reads the path itself.
struct ChatRoomLink: Equatable {
    let productId: String
    let roomId: String
    /// The percent-decoded text after `#`. `nil` when the link has no fragment, or an empty one.
    let payload: String?

    init(productId: String, roomId: String, payload: String? = nil) {
        self.productId = productId
        self.roomId = roomId
        self.payload = payload
    }

    init?(_ link: String) {
        guard let (identifier, path) = Self.productAndPath(of: link) else {
            return nil
        }

        let withoutFragment = path.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        let pathOnly = withoutFragment.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false).first
        let segments = (pathOnly ?? "").split(separator: "/").map(String.init)

        guard
            segments.count == 3,
            segments[0] == "-",
            segments[1] == "chat",
            let roomId = segments[2].removingPercentEncoding?.trimmingCharacters(in: .whitespacesAndNewlines),
            !roomId.isEmpty
        else {
            return nil
        }

        productId = identifier
        self.roomId = roomId
        payload = Self.payload(of: link)
    }

    var chatId: Chat.Id {
        .chatExtension(productId, roomId: roomId)
    }
}

private extension ChatRoomLink {
    /// The product and the path of a dotNS link, or of the web form of a product on a share host.
    static func productAndPath(of link: String) -> (String, String)? {
        if case let .dotName(identifier, path, _) = parseNavigate(input: link) {
            return (identifier, path)
        }

        guard
            let components = URLComponents(string: link),
            let scheme = components.scheme?.lowercased(),
            ["https", "http"].contains(scheme),
            let host = components.host?.lowercased(),
            let shareHost = ProductHost.shareHosts.first(where: { host.hasSuffix("." + $0) })
        else {
            return nil
        }

        let name = host.dropLast(shareHost.count + 1)
        let root = shareHost.split(separator: ".").first.map(String.init) ?? ""
        guard !name.isEmpty, !root.isEmpty else { return nil }

        var path = String(components.percentEncodedPath.drop(while: { $0 == "/" }))
        if let fragment = components.percentEncodedFragment {
            path += "#" + fragment
        }
        return ("\(name).\(root)", path)
    }

    static func payload(of link: String) -> String? {
        guard
            let fragment = link.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).dropFirst().first,
            !fragment.isEmpty
        else {
            return nil
        }

        return String(fragment).removingPercentEncoding ?? String(fragment)
    }
}

/// Opens the room that a room link names. A room that the host does not know yet opens the App of its product,
/// because the product creates its rooms when its worker runs. A link with a payload also gives the payload to the
/// product.
final class ChatRoomLinkService {
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let moduleNavigator: ModuleNavigating
    private let productOpener: @MainActor (String) -> Void
    private let linkOpener: ProductRoomLinkOpening?

    init(
        storageFacade: StorageFacadeProtocol,
        moduleNavigator: ModuleNavigating,
        linkOpener: ProductRoomLinkOpening? = nil,
        productOpener: @escaping @MainActor (String) -> Void = { ProductOpener().open(productId: $0) }
    ) {
        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        )
        self.moduleNavigator = moduleNavigator
        self.linkOpener = linkOpener
        self.productOpener = productOpener
    }
}

extension ChatRoomLinkService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard let link = ChatRoomLink(url.absoluteString) else {
            return false
        }

        Task { @MainActor [chatRepository, moduleNavigator, productOpener, linkOpener] in
            let chat = try? await chatRepository
                .fetchOperation(by: { link.chatId.rawRepresentation }, options: RepositoryFetchOptions())
                .asyncExecute()

            if chat != nil {
                moduleNavigator.openChat(link.chatId)
            } else {
                productOpener(link.productId)
            }

            if let payload = link.payload {
                Logger.shared.debug(
                    "Room link of \(link.productId)/\(link.roomId): payload of \(payload.utf8.count) bytes"
                )
                await linkOpener?.open(link)
            }
        }

        return true
    }
}

/// Gives the payload of a room link to the product of the room.
protocol ProductRoomLinkOpening: Sendable {
    func open(_ link: ChatRoomLink) async
}

/// The product answers `chat.openLink` through its bot, which starts and holds its worker. A product that names
/// another room in its answer gets that room on screen.
final class ProductRoomLinkOpener: ProductRoomLinkOpening, @unchecked Sendable {
    private let registry: ChatExtensionsRegistering
    private let moduleNavigator: ModuleNavigating
    private let logger: LoggerProtocol

    init(
        registry: ChatExtensionsRegistering,
        moduleNavigator: ModuleNavigating,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.registry = registry
        self.moduleNavigator = moduleNavigator
        self.logger = logger
    }

    func open(_ link: ChatRoomLink) async {
        guard let payload = link.payload else { return }

        guard let product = registry.getChatExtensionBot(for: link.productId) as? ProductChatLinkTaking else {
            logger.warning("Room link: no chat product \(link.productId) takes links")
            return
        }

        do {
            let answer = try await product.openLink(
                ProductChatOpenLinkRequest(roomId: link.roomId, payload: payload)
            )
            if let roomId = answer.roomId, roomId != link.roomId {
                await moduleNavigator.openChat(.chatExtension(link.productId, roomId: roomId))
            }
        } catch {
            logger.warning("Room link: \(link.productId) did not take the payload: \(error)")
        }
    }
}

/// A chat product that takes the payload of a room link (`chat.openLink`).
protocol ProductChatLinkTaking: Sendable {
    func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem
}
