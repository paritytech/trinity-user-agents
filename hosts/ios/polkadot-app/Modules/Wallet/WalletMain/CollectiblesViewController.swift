import Products
import SnapKit
import UIKit

/// The Collectibles screen pushed over the Wallet, with the stash product filling it.
final class CollectiblesViewController: UIViewController {
    private let product: SPAViewProtocol

    init(product: SPAViewProtocol) {
        self.product = product

        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        title = String(localized: .Products.collectibles)
        addChild(product.controller)
        view.addSubview(product.controller.view)
        product.controller.view.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        product.controller.didMove(toParent: self)
    }
}

extension CollectiblesViewController: TabBarRootStandIn {}
