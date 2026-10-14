import UIKit
import Products
import UIKitExt

@MainActor
final class ProductsNavigationRouter: ProductsNavigationRouting, WebPresentable {
    private let anchor = ProductsRouter()

    nonisolated init() {}

    func setPresentationView(_ view: ControllerBackedProtocol) {
        anchor.setPresentationView(view)
    }

    var isReady: Bool {
        anchor.isReady
    }

    @discardableResult
    func present(view: ControllerBackedProtocol) -> Bool {
        anchor.present(view: view)
    }

    func navigateTo(destination: ProductPage) async throws {
        guard isReady else {
            throw ProductNativeApiError.navigationForbidden
        }

        UIApplication.shared.mainTabBarController?.openProduct(page: destination)
    }

    func openExternalURL(_ url: URL) async throws {
        guard isReady else {
            throw ProductNativeApiError.navigationForbidden
        }

        guard let scheme = url.scheme?.lowercased() else { return }

        if supportedSafariScheme.contains(scheme), let view = anchor.presentationView {
            showWeb(url: url, from: view, style: WebPresentableStyle(mode: .modal(.fullScreen)))
        } else {
            _ = await UIApplication.shared.open(url)
        }
    }
}

@MainActor
final class ForbiddenNavigationRouter: ProductsNavigationRouting {
    nonisolated init() {}

    func setPresentationView(_: ControllerBackedProtocol) {
        // Navigation is forbidden in this context; there is nothing to anchor.
    }

    var isReady: Bool { false }

    @discardableResult
    func present(view _: ControllerBackedProtocol) -> Bool { false }

    func navigateTo(destination _: ProductPage) async throws {
        throw ProductNativeApiError.navigationForbidden
    }

    func openExternalURL(_: URL) async throws {
        throw ProductNativeApiError.navigationForbidden
    }
}
