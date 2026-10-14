import Foundation
import Operation_iOS
import TrUAPIHost

/// A link into the room of a product: `polkadot://<product_id>.<tld>/-/chat/<room_id>` (02-DESIGN §13 item 15).
///
/// The core classifies the link as a dotNS page, because it does not serve the `chat` target of the reserved `-`
/// segment. So the host reads the path itself.
struct ChatRoomLink: Equatable {
    let productId: String
    let roomId: String

    init(productId: String, roomId: String) {
        self.productId = productId
        self.roomId = roomId
    }

    init?(_ link: String) {
        guard case let .dotName(identifier, path, _) = parseNavigate(input: link) else {
            return nil
        }

        let pathOnly = path.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        let withoutFragment = pathOnly.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).first
        let segments = (withoutFragment ?? "").split(separator: "/").map(String.init)

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
    }

    var chatId: Chat.Id {
        .chatExtension(productId, roomId: roomId)
    }
}

/// Opens the room that a room link names. A room that the host does not know yet opens the App of its product,
/// because the product creates its rooms when its worker runs.
final class ChatRoomLinkService {
    private let chatRepository: AnyDataProviderRepository<Chat.LocalModel>
    private let moduleNavigator: ModuleNavigating
    private let productOpener: @MainActor (String) -> Void

    init(
        storageFacade: StorageFacadeProtocol,
        moduleNavigator: ModuleNavigating,
        productOpener: @escaping @MainActor (String) -> Void = { ProductOpener().open(productId: $0) }
    ) {
        chatRepository = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: AnyCoreDataMapper(ChatModelMapper()))
        )
        self.moduleNavigator = moduleNavigator
        self.productOpener = productOpener
    }
}

extension ChatRoomLinkService: URLHandlingServiceProtocol {
    func handle(url: URL) -> Bool {
        guard let link = ChatRoomLink(url.absoluteString) else {
            return false
        }

        Task { @MainActor [chatRepository, moduleNavigator, productOpener] in
            let chat = try? await chatRepository
                .fetchOperation(by: { link.chatId.rawRepresentation }, options: RepositoryFetchOptions())
                .asyncExecute()

            if chat != nil {
                moduleNavigator.openChat(link.chatId)
            } else {
                productOpener(link.productId)
            }
        }

        return true
    }
}
