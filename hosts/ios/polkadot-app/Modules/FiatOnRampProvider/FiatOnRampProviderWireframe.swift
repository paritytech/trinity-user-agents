import Foundation
import UIKit
import UIKitExt

@MainActor
final class FiatOnRampProviderWireframe: FiatOnRampProviderWireframeProtocol, WebPresentable {
    func showWidget(url: URL, from view: FiatOnRampProviderViewProtocol?) {
        guard let view else {
            return
        }
        showWeb(
            url: url,
            from: view,
            style: WebPresentableStyle(mode: .modal(.fullScreen))
        )

        DispatchQueue.main.async {
            view.controller.navigationController?.popToRootViewController(animated: true)
        }
    }
}
