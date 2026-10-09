import Foundation
import UIKit.UIImage

@MainActor
final class IdentityDetailsPresenter {
    weak var view: IdentityDetailsViewProtocol?

    let wireframe: IdentityDetailsWireframeProtocol
    let interactor: IdentityDetailsInteractorInputProtocol
    let logger: LoggerProtocol

    private var username: Username?
    private var qrCode: UIImage?

    init(
        interactor: IdentityDetailsInteractorInputProtocol,
        wireframe: IdentityDetailsWireframeProtocol,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.logger = logger
    }
}

extension IdentityDetailsPresenter: IdentityDetailsPresenterProtocol {
    func setup() {
        interactor.setup()
        interactor.generateQrCode(for: CGSize(width: 200, height: 200))
    }

    func onCopyUsername() {
        guard let username else {
            return
        }
        wireframe.copyAddress(from: view, address: username.value, locale: .current)
    }

    func onShare() {
        guard let qrCode, let username else {
            return
        }
        let items = interactor.shareMessage(username: username, image: qrCode)
        wireframe.share(items: items, from: view, with: nil)
    }

    func onQrCode() {
        wireframe.presentQrSheet(from: view)
    }
}

extension IdentityDetailsPresenter: IdentityDetailsInteractorOutputProtocol {
    func didReceive(qrCode: UIImage) {
        self.qrCode = qrCode
        view?.didReceive(qrCode: qrCode)
    }

    func didReceive(profile: IdentityProfile) {
        username = profile.username
        view?.didReceive(isPerson: profile.rank == .membership)

        guard let username = profile.username else {
            return
        }
        view?.didReceive(username: username, claimed: profile.isClaimed)
    }
}
