import Foundation
import UIKit

@MainActor
final class ContactsListWireframe {
    let flowState: ChatFlowState

    private let moduleNavigator: ModuleNavigating

    init(flowState: ChatFlowState, moduleNavigator: ModuleNavigating = ModuleNavigator()) {
        self.flowState = flowState
        self.moduleNavigator = moduleNavigator
    }
}

private extension ContactsListWireframe {
    func performChatShow(from view: ContactsListViewProtocol?, for model: ChatOpenModel) {
        guard let chat = ChatViewFactory.createChatView(with: model, flowState: flowState) else {
            return
        }

        view?.controller.navigationController?.pushViewController(chat.controller, animated: true)
    }
}

extension ContactsListWireframe: ContactsListWireframeProtocol {
    func showScanPanel() {
        moduleNavigator.openScanPanel()
    }

    func showChat(from view: ContactsListViewProtocol?, for model: ChatOpenModel) {
        performChatShow(from: view, for: model)
    }

    func showChat(from view: ContactsListViewProtocol?, for model: ChatOpenModel, showing messageId: Chat.MessageId) {
        guard let chat = ChatViewFactory.createChatView(with: model, flowState: flowState) else {
            return
        }

        view?.controller.navigationController?.pushViewController(chat.controller, animated: true)
        // The room screen scrolls to the message when the message arrives in its window. A product-source room
        // pages to the message, because its window holds only the newest page at first.
        (chat as? ChatProductRoomViewProtocol)?.scrollToMessage(messageId)
        ((chat as? ChatViewController)?.presenter as? ChatPresenter)?.productRoom?.showListSearchHit(messageId)
    }

    func open(url: URL) {
        UIApplication.shared.open(url)
    }

    func showIncomingRequests(from view: ContactsListViewProtocol?) {
        guard let chatRequestList = ChatRequestListViewFactory.createView(for: flowState) else {
            return
        }

        chatRequestList.controller.hidesBottomBarWhenPushed = true

        view?.controller.navigationController?.pushViewController(
            chatRequestList.controller,
            animated: true
        )
    }

    func showArchivedChats(from view: ContactsListViewProtocol?) {
        guard let archived = ChatArchiveViewFactory.createView(flowState: flowState) else {
            return
        }

        view?.controller.navigationController?.pushViewController(archived.controller, animated: true)
    }

    func showMuteOptions(from view: ContactsListViewProtocol?, onSelect: @escaping (Chat.MuteDuration) -> Void) {
        view?.controller.present(ChatMuteOptions.sheet(onSelect: onSelect), animated: true)
    }
}
