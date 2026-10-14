import UIKit
import PolkadotUI

final class PolkadotSigningDetailsPresenter {
    weak var view: PolkadotSigningDetailsViewProtocol?

    private let wireframe: PolkadotSigningDetailsWireframeProtocol
    private let detailsText: String
    private let isTransaction: Bool

    init(
        wireframe: PolkadotSigningDetailsWireframeProtocol,
        detailsText: String,
        isTransaction: Bool
    ) {
        self.wireframe = wireframe
        self.detailsText = detailsText
        self.isTransaction = isTransaction
    }
}

extension PolkadotSigningDetailsPresenter: PolkadotSigningDetailsPresenterProtocol {
    func setup() {
        view?.didReceive(viewModel: .init(
            text: detailsText,
            isTransaction: isTransaction
        ))
    }
}
