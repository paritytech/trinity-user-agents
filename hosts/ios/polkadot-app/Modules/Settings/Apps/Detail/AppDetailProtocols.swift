import Foundation
import PolkadotUI
import Products
import UIKitExt

protocol AppDetailViewProtocol: ControllerBackedProtocol {
    func didReceive(name: String, subtitle: String?)
    func didReceive(avatar: AvatarViewModel)
    func didReceive(icon: any ImageViewModelProtocol)
}

@MainActor
protocol AppDetailPresenterProtocol: AnyObject {
    func setup()
    func didTapPermissions()
}

protocol AppDetailInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol AppDetailInteractorOutputProtocol: AnyObject {
    func didReceive(product: ResolvedProduct)
}

@MainActor
protocol AppDetailWireframeProtocol: AnyObject {
    func showPermissions(
        productId: ProductId,
        productName: String,
        from view: AppDetailViewProtocol?
    )
}
