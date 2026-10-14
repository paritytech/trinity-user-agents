import Foundation
import SubstrateSdk
import ChainRegistry

class SelectTokenPresenter: TokensPresenter {
    weak var view: SelectTokenViewProtocol?
    let wireframe: SelectTokenWireframeProtocol

    var interactor: SelectTokenInteractorInputProtocol? {
        tokensInteractor as? SelectTokenInteractorInputProtocol
    }

    let viewModelFactory: SelectTokenViewModelFactoryProtocol

    init(
        interactor: SelectTokenInteractorInputProtocol,
        wireframe: SelectTokenWireframeProtocol,
        viewModelFactory: SelectTokenViewModelFactoryProtocol,
        logger: LoggerProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory

        super.init(tokensInteractor: interactor, logger: logger)
    }

    private func provideViewModel() {
        let viewModels = (chainAssets ?? []).map { viewModelFactory.createViewModel(from: $0) }

        view?.didReceive(
            viewModels: viewModels // + [.fiat] Fiat onRamp is disabled for the time being
        )
    }

    override func didReceive(chainAssets: [ChainAsset]) {
        super.didReceive(chainAssets: chainAssets)

        provideViewModel()
    }
}

@MainActor
extension SelectTokenPresenter: SelectTokenPresenterProtocol {
    func setup() {
        interactor?.setup()
    }

    func select(viewModel: SelectTokenCellViewModel) {
        switch viewModel {
        case let .chainAsset(asset):
            guard let chainAsset = chainAsset(id: asset.chainAssetId) else { return }
            wireframe.proceed(from: view, chainAsset: chainAsset)

        case .fiat:
            wireframe.proceedToFiatOnRamp(from: view)
        }
    }

    private func chainAsset(id: ChainAssetId) -> ChainAsset? {
        chainAssets?.first { $0.chainAssetId == id }
    }
}

extension SelectTokenPresenter: SelectTokenInteractorOutputProtocol {}
