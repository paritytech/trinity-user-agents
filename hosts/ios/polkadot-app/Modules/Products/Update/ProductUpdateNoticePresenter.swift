import UIKit

@MainActor
protocol ProductUpdateNoticePresenting: AnyObject {
    func show(productName: String, onUpdate: @escaping () -> Void, onDismiss: @escaping () -> Void)
    func hide()
}

@MainActor
final class ProductUpdateNoticePresenter: ProductUpdateNoticePresenting {
    private let application: UIApplication
    private var window: TopNoticeWindow?

    init(application: UIApplication) {
        self.application = application
    }

    func show(productName: String, onUpdate: @escaping () -> Void, onDismiss: @escaping () -> Void) {
        guard let windowScene = foregroundWindowScene() else { return }

        let notice = window ?? TopNoticeWindow(windowScene: windowScene)
        window = notice
        notice.show(ProductUpdateNoticeView(productName: productName, onUpdate: onUpdate, onDismiss: onDismiss))
    }

    func hide() {
        window?.hide()
    }
}

private extension ProductUpdateNoticePresenter {
    func foregroundWindowScene() -> UIWindowScene? {
        let scenes = application.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
    }
}
