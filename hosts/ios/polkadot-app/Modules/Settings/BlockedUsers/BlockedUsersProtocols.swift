import PolkadotUI
import SubstrateSdk
import UIKitExt

protocol BlockedUsersViewProtocol: ControllerBackedProtocol {
    func didReceive(items: [BlockedUsersViewLayout.Item])
}

@MainActor
protocol BlockedUsersPresenterProtocol: AnyObject {
    func setup()
    func selectUser(_ item: BlockedUsersViewLayout.Item)
    func unblockUser(_ item: BlockedUsersViewLayout.Item)
}

protocol BlockedUsersInteractorInputProtocol: AnyObject {
    func setup()
    func unblockUser(accountId: AccountId)
}

@MainActor
protocol BlockedUsersInteractorOutputProtocol: AnyObject {
    func didReceive(contactsById: [String: Chat.Contact])
}

@MainActor
protocol BlockedUsersWireframeProtocol: AnyObject, ChatNavigating {}
