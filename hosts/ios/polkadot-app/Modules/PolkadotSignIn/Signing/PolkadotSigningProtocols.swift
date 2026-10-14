import PolkadotUI
import UIKitExt

protocol PolkadotSigningViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: PolkadotSigningViewLayout.ViewModel)
}

@MainActor
protocol PolkadotSigningPresenterProtocol: AnyObject {
    func setup()
    func sign()
    func cancel()
    func viewDetails()
}

protocol PolkadotSigningInteractorInputProtocol: AnyObject {
    func setup()
    func signParsedResult(_ parsedResult: PolkadotParsedSigningRequestResult)
}

@MainActor
protocol PolkadotSigningInteractorOutputProtocol: AnyObject {
    func didStartParsingRequest()
    func didFinishParsingRequest(with result: PolkadotParsedSigningRequestResult)
    func didFailToParseRequest(with error: Error)

    func didStartSigning()
    func didFinishSigning(with result: PolkadotHostSigningResult)
    func didFailToSign(with error: Error)
}

/// Shared presentation for both the interactive signing flow and the rust-core
/// confirm-only flow: view-details navigation plus alert/error surfaces.
@MainActor
protocol PolkadotSigningDetailsPresentable: AlertPresentable, ErrorPresentable {
    func showViewDetails(
        with text: String,
        isTransaction: Bool,
        view: PolkadotSigningViewProtocol?
    )
}

extension PolkadotSigningDetailsPresentable {
    func showViewDetails(
        with text: String,
        isTransaction: Bool,
        view: PolkadotSigningViewProtocol?
    ) {
        guard let detailsView = PolkadotSigningDetailsViewFactory.createView(
            detailsText: text,
            isTransaction: isTransaction
        ) else {
            return
        }
        let nav = AppNavigationController(rootViewController: detailsView.controller)
        view?.controller.present(nav, animated: true)
    }
}

@MainActor
protocol PolkadotSigningWireframeProtocol: PolkadotSigningDetailsPresentable {
    func hide(view: PolkadotSigningViewProtocol?, decision: PolkadotSigningDecision)
}
