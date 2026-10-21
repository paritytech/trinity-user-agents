import Foundation
import UIKit

/// The search action of the room header (P10.5, 07-LATER §8.2). A host-store room searches the store of the host.
/// A product-source room searches through its product, and only when it declares `search`.
extension ChatProductRoomPresenter {
    /// Draws the search action of the header, or removes it.
    func provideSearchAction() {
        let offersSearch = policy?.offersSearch == true
        mark("search-action", offersSearch ? "shown" : "hidden")

        guard offersSearch else {
            view?.didReceive(searchItem: nil)
            return
        }

        let action = UIAction(
            title: String(localized: .ChatExtension.roomSearch),
            image: UIImage(systemName: "magnifyingglass")
        ) { [weak self] _ in
            self?.showSearch()
        }
        let item = UIBarButtonItem(primaryAction: action)
        item.tintColor = .fgPrimary
        view?.didReceive(searchItem: item)
    }

    func showSearch() {
        guard let source = searchSource() else { return }

        let search = ChatSearchViewFactory.createView(
            source: source,
            roomId: roomId,
            peerName: appearanceFactory.productName
        ) { [weak self] messageId in
            self?.showSearchResult(messageId)
        }
        view?.controller.navigationController?.pushViewController(search.controller, animated: true)
    }

    /// Shows a message that the search found. A host-store room has every message in its window. A product-source
    /// room pages to the message, as a jump to a reply quote does.
    func showSearchResult(_ messageId: Chat.MessageId) {
        switch policy?.messageStore {
        case .product:
            interactor?.jump(to: messageId)
        case .host,
             nil:
            markJump(to: messageId, found: messages[messageId] != nil)
            view?.scrollToMessage(messageId)
        }
    }

    /// Shows a message that the search of the chat list found. The room asks its source to page to the message.
    /// The source of a host-store room has every message already.
    func showListSearchHit(_ messageId: Chat.MessageId) {
        interactor?.jump(to: messageId)
    }

    /// The E2E harness reads where a jump went (S17, S61, S62).
    func markJump(to messageId: Chat.MessageId, found: Bool) {
        mark("jumped", "\(productMessageId(messageId)) found=\(found)")
    }
}

private extension ChatProductRoomPresenter {
    func searchSource() -> ChatRoomSearching? {
        switch policy?.messageStore {
        case .product:
            guard let searching = room as? ProductChatSearching else { return nil }
            return ProductChatRoomSearch(chatId: chatId, roomId: roomId, searching: searching, logger: logger)
        case .host:
            return StoreChatRoomSearch(chatId: chatId, service: searchService)
        case nil:
            return nil
        }
    }
}

extension ChatRoomPolicy {
    /// The room header shows the search action: always for a room that the host stores, and for a product-source
    /// room that declares `search`.
    var offersSearch: Bool {
        messageStore == .host || features.search
    }
}
