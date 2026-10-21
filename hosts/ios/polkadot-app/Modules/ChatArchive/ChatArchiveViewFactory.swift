import Foundation

/// The list of the product rooms that the user archived (07-LATER §6.2). It is the chat list in its archived scope:
/// the same rows, swipe actions and menus, without the "Archived" row and without the search field.
@MainActor
enum ChatArchiveViewFactory {
    static func createView(flowState: ChatFlowState) -> ContactsListViewProtocol? {
        let view = ContactsListViewFactory.createView(flowState: flowState, scope: .archived)
        view?.controller.hidesBottomBarWhenPushed = true
        return view
    }
}
