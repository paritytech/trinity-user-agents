import UIKit
import UIKitExt

@MainActor
final class ShareWireframe: ShareWireframeProtocol {
    private weak var host: ControllerBackedProtocol?
    private let composer: ShareContentComposing

    init(host: ControllerBackedProtocol?, composer: ShareContentComposing) {
        self.host = host
        self.composer = composer
    }

    func close(from view: ControllerBackedProtocol?) {
        view?.controller.dismiss(animated: true)
    }

    func presentSystemShare(items: [ShareItem], from view: ControllerBackedProtocol?) {
        let activityItems = composer.toActivityItems(items)
        view?.controller.dismiss(animated: true) { [weak host] in
            guard let host else { return }
            let adapter = ShareActivityAdapter()
            adapter.use(presenter: host)
            adapter.share(activityItems: activityItems) { _ in }
        }
    }
}
