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
}
