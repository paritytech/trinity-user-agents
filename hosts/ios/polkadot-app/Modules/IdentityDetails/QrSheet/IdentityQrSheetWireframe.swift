import UIKit

final class IdentityQrSheetWireframe: IdentityQrSheetWireframeProtocol {
    func close(from view: IdentityQrSheetViewProtocol?) {
        view?.controller.dismiss(animated: true, completion: nil)
    }
}
