import DesignSystem
import Operation_iOS
import Products
import SwiftUI
import TrUAPIHost
import UIKit
import UIKitExt

final class DebugSettingsWireframe: DebugSettingsWireframeProtocol {
    private let flowStateProvider: any SPAFlowStateProviding

    init(flowStateProvider: any SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func showProducts(from view: ControllerBackedProtocol?) {
        let factory = ProductRepositoryFactory()

        let viewModel = DebugProductsViewModel(
            productRepository: factory.createRepository(),
            chatRepositoryFactory: ChatRepositoryFactory()
        )

        let productsView = DebugProductsListView(viewModel: viewModel)
        let hostingController = UIHostingController(rootView: productsView)

        view?.controller.navigationController?.pushViewController(hostingController, animated: true)
    }

    /// The face loop: a URL, a decode and a draw, with no worker, no manifest
    /// and no product in the way.
    func showPocketFacePreview(from view: ControllerBackedProtocol?) {
        let controller = UIHostingController(rootView: DebugPocketFacePreviewView())
        view?.controller.navigationController?.pushViewController(controller, animated: true)
    }

    func showPocketCards(from view: ControllerBackedProtocol?) {
        let controller = UIHostingController(rootView: DebugPocketCardsView())
        view?.controller.navigationController?.pushViewController(controller, animated: true)
    }

    func showThemeSelection(from view: ControllerBackedProtocol?) {
        let themeView = DebugThemeSelectionView(
            themeManager: ThemeManager.shared,
            typographyManager: TypographyManager.shared
        )
        let hostingController = UIHostingController(rootView: themeView)
        hostingController.title = "Theme Selection"

        view?.controller.navigationController?.pushViewController(hostingController, animated: true)
    }

    func showTrUAPIPlayground(from view: ControllerBackedProtocol?) {
        #if DEBUG
            guard let playgroundView = TrUAPIPlaygroundViewFactory.createView(
                flowStateProvider: flowStateProvider
            ) else {
                return
            }

            let navigationController = AppNavigationController(
                rootViewController: playgroundView.controller
            )
            navigationController.modalPresentationStyle = .fullScreen

            view?.controller.present(navigationController, animated: true)
        #endif
    }

    func showDevServer(from view: ControllerBackedProtocol?) {
        #if DEBUG
            let alert = UIAlertController(
                title: "Open dev server",
                message: "localhost:3000 on the simulator, the Mac's address (192.168.x.x:3000) on a device",
                preferredStyle: .alert
            )

            alert.addTextField { textField in
                textField.placeholder = "localhost:3000"
                textField.keyboardType = .URL
                textField.autocapitalizationType = .none
                textField.autocorrectionType = .no
            }

            alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
            alert.addAction(UIAlertAction(title: "Open", style: .default) { [weak view, weak self] _ in
                guard let self else {
                    return
                }

                let input = alert.textFields?.first?.text ?? ""

                guard let product = parseDevServer(input: input) else {
                    present(
                        message: "Not a development server address: \(input)",
                        title: "Cannot open",
                        closeAction: "Close",
                        from: view
                    )
                    return
                }

                Task { @MainActor [flowStateProvider] in
                    guard let spaView = await DevServerViewFactory.createView(
                        product: product,
                        flowStateProvider: flowStateProvider
                    ) else {
                        return
                    }

                    view?.controller.navigationController?.pushViewController(spaView.controller, animated: true)
                }
            })

            view?.controller.present(alert, animated: true)
        #endif
    }

    func showDotNsBrowser(from view: ControllerBackedProtocol?) {
        let alert = UIAlertController(
            title: "Open SPA",
            message: "Enter a dotns name to open",
            preferredStyle: .alert
        )

        alert.addTextField { textField in
            textField.placeholder = "browse.dot"
        }

        alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        alert.addAction(UIAlertAction(title: "Open", style: .default) { [weak view, weak self] _ in
            guard
                let input = alert.textFields?.first?.text,
                let self
            else {
                return
            }

            let flowState = flowStateProvider.flowState()

            Task {
                guard
                    let productHost = try? await flowState.hostProvider.resolveHost(rawString: input),
                    let spaView = SPAViewFactory.createView(
                        page: ProductPage(host: productHost),
                        flowState: flowState
                    )
                else {
                    return
                }

                await MainActor.run {
                    view?.controller.navigationController?.pushViewController(
                        spaView.controller,
                        animated: true
                    )
                }
            }
        })

        view?.controller.present(alert, animated: true)
    }
}
