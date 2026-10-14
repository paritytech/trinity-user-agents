import Foundation

final class SPAMoreActionsPresenter: SPAMoreActionsPresenterProtocol {
    weak var view: SPAMoreActionsViewProtocol?

    let actions: [SPAMoreAction]
    let closeTitle: String

    init(
        actions: [SPAMoreAction],
        closeTitle: String
    ) {
        self.actions = actions
        self.closeTitle = closeTitle
    }

    func didSelectAction(at index: Int) {
        guard actions.indices.contains(index), actions[index].isEnabled else { return }

        let handler = actions[index].handler
        view?.controller.dismiss(animated: true) {
            handler()
        }
    }

    func didSelectClose() {
        view?.controller.dismiss(animated: true)
    }
}
