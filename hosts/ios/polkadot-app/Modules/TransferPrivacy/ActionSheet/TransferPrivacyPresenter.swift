import Foundation

final class TransferPrivacyPresenter {
    weak var view: TransferPrivacyViewProtocol?

    private let model: TransferPrivacyModel
    private let wireframe: TransferPrivacyWireframeProtocol
    private let onSendAnyway: () -> Void
    private let onCancel: () -> Void

    init(
        model: TransferPrivacyModel,
        wireframe: TransferPrivacyWireframeProtocol,
        onSendAnyway: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.model = model
        self.wireframe = wireframe
        self.onSendAnyway = onSendAnyway
        self.onCancel = onCancel
    }
}

extension TransferPrivacyPresenter: TransferPrivacyPresenterProtocol {
    func setup() {
        let viewModel = TransferPrivacyViewModel(
            title: String(localized: .Transfer.privacyConfirmTitle),
            message: String(localized: .Transfer.privacyConfirmBody),
            sendAnywayTitle: String(localized: .Transfer.privacyConfirmSendAnyway(model.amount))
        )
        view?.didReceive(viewModel: viewModel)
    }

    func sendAnyway() {
        wireframe.complete(from: view) { [onSendAnyway] in onSendAnyway() }
    }

    func cancel() {
        wireframe.complete(from: view) { [onCancel] in onCancel() }
    }
}
