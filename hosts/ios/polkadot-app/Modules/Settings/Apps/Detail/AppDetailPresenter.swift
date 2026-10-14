import Foundation
import PolkadotUI
import Products

@MainActor
final class AppDetailPresenter {
    weak var view: AppDetailViewProtocol?

    private let wireframe: AppDetailWireframeProtocol
    private let interactor: AppDetailInteractorInputProtocol
    private let iconViewModelFactory: ProductIconViewModelMaking
    private let productId: ProductId

    private var product: ResolvedProduct?

    init(
        productId: ProductId,
        interactor: AppDetailInteractorInputProtocol,
        wireframe: AppDetailWireframeProtocol,
        iconViewModelFactory: ProductIconViewModelMaking
    ) {
        self.productId = productId
        self.interactor = interactor
        self.wireframe = wireframe
        self.iconViewModelFactory = iconViewModelFactory
    }
}

extension AppDetailPresenter: AppDetailPresenterProtocol {
    func setup() {
        // The icon is keyed by domain alone, so it starts loading without waiting for the manifest.
        view?.didReceive(name: productId, subtitle: nil)
        view?.didReceive(icon: iconViewModelFactory.createViewModel(for: productId))

        interactor.setup()
    }

    func didTapPermissions() {
        wireframe.showPermissions(
            productId: productId,
            productName: product?.displayName ?? productId,
            from: view
        )
    }
}

extension AppDetailPresenter: AppDetailInteractorOutputProtocol {
    func didReceive(product: ResolvedProduct) {
        self.product = product

        view?.didReceive(name: product.displayName, subtitle: product.domainSubtitle)
        view?.didReceive(avatar: product.placeholderAvatar)
    }
}
