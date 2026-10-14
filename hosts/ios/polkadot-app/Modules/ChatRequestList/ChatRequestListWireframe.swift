import Foundation

final class ChatRequestListWireframe: ChatRequestListWireframeProtocol {
    let flowState: ChatFlowState

    init(flowState: ChatFlowState) {
        self.flowState = flowState
    }

    func showChat(
        from view: ChatRequestListViewProtocol?,
        openModel: ChatOpenModel
    ) {
        guard
            let chatView = ChatViewFactory.createChatView(
                with: openModel,
                flowState: flowState
            )
        else {
            return
        }

        view?.controller.navigationController?.pushViewController(
            chatView.controller,
            animated: true
        )
    }

    func close(from view: ChatRequestListViewProtocol?) {
        guard let navigationController = view?.controller.navigationController else { return }

        if navigationController.topViewController === view?.controller {
            navigationController.popViewController(animated: true)
        } else {
            let updatedStack = navigationController.viewControllers.filter { $0 !== view?.controller }
            navigationController.setViewControllers(updatedStack, animated: false)
        }
    }
}
