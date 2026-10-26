import SnapKit
import SwiftUI
import UIKit

/// A pass-through window pinned to the top of the screen for notices that must stay visible over
/// whatever the app presents.
final class TopNoticeWindow: UIWindow {
    private let hostingController: UIHostingController<AnyView> = {
        let controller = UIHostingController(rootView: AnyView(EmptyView()))
        controller.sizingOptions = [.intrinsicContentSize]
        return controller
    }()

    override init(windowScene: UIWindowScene) {
        super.init(windowScene: windowScene)

        windowLevel = .alert - 1
        rootViewController = UIViewController()
        rootViewController?.view.backgroundColor = .clear
        backgroundColor = .clear

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // A window's root view can't be used for hit-test passthrough: when the root view's
    // hitTest returns nil, UIKit falls back to the window itself, whose own hitTest still
    // returns self for any point inside its bounds (UIWindow is a UIView). The passthrough
    // must therefore be implemented here, on the window, not on the root view.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)

        guard hitView !== self, hitView !== rootViewController?.view else {
            return nil
        }

        return hitView
    }

    func show(_ content: some View) {
        hostingController.rootView = AnyView(content)
        isHidden = false
    }

    func hide() {
        isHidden = true
    }
}

private extension TopNoticeWindow {
    func setupLayout() {
        guard let rootViewController else { return }

        rootViewController.addChild(hostingController)
        rootViewController.view.addSubview(hostingController.view)
        hostingController.didMove(toParent: rootViewController)

        hostingController.view.backgroundColor = .clear
        hostingController.view.snp.makeConstraints { make in
            make.top.equalTo(rootViewController.view.safeAreaLayoutGuide.snp.top).offset(20)
            make.leading.equalToSuperview().offset(UIConstants.horizontalInsetMedium)
            make.trailing.equalToSuperview().inset(UIConstants.horizontalInsetMedium)
        }
    }
}
