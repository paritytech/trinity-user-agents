import Foundation
import UIKit.UIImage
import UIKitExt
import PolkadotUI

protocol IdentityDetailsViewProtocol: ControllerBackedProtocol {
    var viewModel: IdentityDetailsViewModel { get }
    func didReceive(username: Username, claimed: Bool)
    func didReceive(qrCode: UIImage)
}

@MainActor
protocol IdentityDetailsPresenterProtocol: AnyObject {
    func setup()
    func onCopyUsername()
    func onShare()
    func onQrCode()
}

protocol IdentityDetailsInteractorInputProtocol: AnyObject {
    func setup()
    func shareMessage(username: Username, image: UIImage) -> [Any]
    func generateQrCode(for size: CGSize)
}

@MainActor
protocol IdentityDetailsInteractorOutputProtocol: AnyObject {
    func didReceive(qrCode: UIImage)
    func didReceive(profile: IdentityProfile)
}

@MainActor
protocol IdentityDetailsWireframeProtocol: AnyObject,
    SharingPresentable,
    AddressCopyPresentable {
    func presentQrSheet(from view: IdentityDetailsViewProtocol?)
}
