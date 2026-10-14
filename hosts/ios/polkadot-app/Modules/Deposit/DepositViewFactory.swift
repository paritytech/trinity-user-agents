import Foundation
import UIKit
import SubstrateSdk
import ChainRegistry

@MainActor
enum DepositViewFactory {
    static func createView(
        chainAsset: ChainAsset,
        depositService: DepositServiceProtocol,
        completion: @escaping () -> Void
    ) -> DepositViewProtocol? {
        let fundedAsset = AppConfig.Assets.fundedAsset
        guard
            let fundedChain = ChainRegistryFacade.sharedRegistry.getChain(
                for: fundedAsset.chainId
            ),
            let fundedAsset = fundedChain.chainAsset(
                for: fundedAsset.assetId
            )
        else {
            return nil
        }

        let qrEncoder = AddressQREncoder(
            addressFormat: .substrate(type: chainAsset.chain.addressPrefix)
        )

        let interactor = DepositInteractor(
            depositService: depositService,
            depositAsset: chainAsset,
            fundedAsset: fundedAsset,
            chainRegistry: ChainRegistryFacade.sharedRegistry,
            qrEncoder: qrEncoder
        )

        let wireframe = DepositWireframe(completion: completion)

        let presenter = DepositPresenter(
            interactor: interactor,
            wireframe: wireframe,
            assetIn: chainAsset,
            assetOut: fundedAsset,
            viewModelFactory: DepositViewModelFactory()
        )

        let view = DepositViewController(presenter: presenter)

        presenter.view = view
        interactor.presenter = presenter

        return view
    }
}
