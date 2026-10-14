import Foundation
import PolkadotUI
import Products
import UIKitExt

protocol AppsListViewProtocol: ControllerBackedProtocol {
    func didReceive(items: [AppsListViewLayout.Item])
}

@MainActor
protocol AppsListPresenterProtocol: AnyObject {
    func setup()
    func selectApp(_ item: AppsListViewLayout.Item)
}

protocol AppsListInteractorInputProtocol: AnyObject {
    func setup()
}

@MainActor
protocol AppsListInteractorOutputProtocol: AnyObject {
    func didReceive(products: [ResolvedProduct])
}

@MainActor
protocol AppsListWireframeProtocol: AnyObject {
    func showAppDetail(productId: ProductId, from view: AppsListViewProtocol?)
}
