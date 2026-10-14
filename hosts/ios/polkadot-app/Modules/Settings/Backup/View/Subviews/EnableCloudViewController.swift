import UIKit

final class EnableCloudViewController: UIViewController {
    private let rootView = EnableCloudViewLayout()

    override func loadView() {
        view = rootView
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.openSettingsButton.addAction(
            UIAction { [weak self] _ in
                self?.dismiss(animated: true) {
                    guard let settingsUrl = URL(string: UIApplication.openSettingsURLString) else {
                        return
                    }
                    UIApplication.shared.open(settingsUrl)
                }
            },
            for: .touchUpInside
        )
    }
}
