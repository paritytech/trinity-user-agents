import UIKit
import UIKitExt

@MainActor
protocol FundingDepositViewProtocol: ControllerBackedProtocol {
    /// `qrCode` is the deposit's QR code once drawn.
    func didReceive(viewModel: FundingDepositViewModel, qrCode: UIImage?)
}

@MainActor
protocol FundingDepositPresenterProtocol: AnyObject {
    func setup()
    func back()
    func showProgress()
    func confirmCancel()
    /// The user copied one of the deposit details.
    func didCopy()
    func close()
}

@MainActor
protocol FundingDepositInteractorInputProtocol: AnyObject {
    func setup()
    /// The user left before the session started: the core no longer starts
    /// it on its best quote.
    func stopStarting()
    /// Draws the QR code carrying `payload`.
    func createQRCode(for payload: String)
}

@MainActor
protocol FundingDepositInteractorOutputProtocol: AnyObject {
    func didReceive(qrCode: UIImage, for payload: String)
}

@MainActor
protocol FundingDepositWireframeProtocol: FundingToastPresentable {
    func show(_ screen: FundingFlowScreen)
    func showProgress()
    func back()
    func close(then completion: (() -> Void)?)
}

extension FundingFlowWireframe: FundingDepositWireframeProtocol {}

enum FundingDepositConstants {
    /// The QR code's side on screen, which the interactor draws it at.
    static let qrSide: CGFloat = 220
}
