import UIKitExt
import ChainRegistry

protocol SelectTokenViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModels: [SelectTokenCellViewModel])
}

@MainActor
protocol SelectTokenPresenterProtocol: AnyObject {
    func setup()
    func select(viewModel: SelectTokenCellViewModel)
}

protocol SelectTokenInteractorInputProtocol: TokensInputProtocol {}

@MainActor
protocol SelectTokenInteractorOutputProtocol: TokensOutputProtocol {}

@MainActor
protocol SelectTokenWireframeProtocol: AnyObject {
    func proceed(from view: SelectTokenViewProtocol?, chainAsset: ChainAsset)
    func proceedToFiatOnRamp(from view: SelectTokenViewProtocol?)
}
