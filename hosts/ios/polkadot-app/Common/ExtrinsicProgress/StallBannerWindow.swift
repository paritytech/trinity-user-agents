#if TESTNET_FEATURE
    import UIKit
    import SnapKit
    import SwiftUI
    import PolkadotUI

    final class StallBannerWindow: UIWindow {
        private let bannerHostingController: UIHostingController<StallBannerView> = {
            let controller = UIHostingController(
                rootView: StallBannerView(
                    viewModel: .init(transactions: [], overflowText: nil),
                    onDismiss: { _ in }
                )
            )
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

        func show(viewModel: StallBannerViewModel, onDismiss: @escaping (UUID) -> Void) {
            bannerHostingController.rootView = StallBannerView(viewModel: viewModel, onDismiss: onDismiss)
            isHidden = false
        }

        func hide() {
            isHidden = true
        }
    }

    private extension StallBannerWindow {
        func setupLayout() {
            guard let rootViewController else { return }

            rootViewController.addChild(bannerHostingController)
            rootViewController.view.addSubview(bannerHostingController.view)
            bannerHostingController.didMove(toParent: rootViewController)

            bannerHostingController.view.backgroundColor = .clear
            bannerHostingController.view.snp.makeConstraints { make in
                make.top.equalTo(rootViewController.view.safeAreaLayoutGuide.snp.top).offset(20)
                make.leading.equalToSuperview().offset(UIConstants.horizontalInsetMedium)
                make.trailing.equalToSuperview().inset(UIConstants.horizontalInsetMedium)
            }
        }
    }
#endif
