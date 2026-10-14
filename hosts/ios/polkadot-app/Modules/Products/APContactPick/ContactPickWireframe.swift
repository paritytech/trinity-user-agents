import Foundation
import SubstrateSdk

/// Ends the search by answering the product rather than opening a chat.
///
/// The shared screen resolves a selection into a `ChatOpenModel`; for a pick
/// the only part that matters is who was chosen. Only `.person` can arrive:
/// the provider serves stored contacts alone, and every one of those is a
/// person the user already has a chat identity for.
@MainActor
final class ContactPickWireframe: SearchContactWireframeProtocol {
    private let context: ContactPickContext

    init(context: ContactPickContext) {
        self.context = context
    }

    func complete(from view: SearchContactViewProtocol?, with model: ChatOpenModel) {
        let picked: AccountId? =
            switch model {
            case let .existingChat(.person(accountId)): accountId
            case .existingChat(.chatExtension),
                 .newRequest: nil
            }

        view?.controller.dismiss(animated: true) { [context] in
            MainActor.assumeIsolated { context.deliver(picked) }
        }
    }
}
