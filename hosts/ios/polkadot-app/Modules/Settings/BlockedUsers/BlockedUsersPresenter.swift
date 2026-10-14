import Foundation
import PolkadotUI

final class BlockedUsersPresenter {
    weak var view: BlockedUsersViewProtocol?

    private let wireframe: BlockedUsersWireframeProtocol
    private let interactor: BlockedUsersInteractorInputProtocol

    private var contactsById: [String: Chat.Contact] = [:]

    init(
        interactor: BlockedUsersInteractorInputProtocol,
        wireframe: BlockedUsersWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension BlockedUsersPresenter: BlockedUsersPresenterProtocol {
    func setup() {
        interactor.setup()
    }

    func selectUser(_ item: BlockedUsersViewLayout.Item) {
        guard let contact = contactsById[item.id] else {
            return
        }

        wireframe.navigateToChat(with: .person(contact.accountId), force: false)
    }

    func unblockUser(_ item: BlockedUsersViewLayout.Item) {
        guard let contact = contactsById[item.id] else {
            return
        }

        interactor.unblockUser(accountId: contact.accountId)
    }
}

extension BlockedUsersPresenter: BlockedUsersInteractorOutputProtocol {
    func didReceive(contactsById: [String: Chat.Contact]) {
        self.contactsById = contactsById

        let items = contactsById.values.map {
            BlockedUsersViewLayout.Item(id: $0.identifier, username: $0.username)
        }

        view?.didReceive(items: items)
    }
}
