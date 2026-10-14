import Observation

protocol SelectTokenViewModelProtocol: Observation.Observable {
    var viewModels: [SelectTokenCellViewModel] { get set }
    var onTap: ((SelectTokenCellViewModel) -> Void)? { get set }
}

@Observable
final class SelectTokenViewModel: SelectTokenViewModelProtocol {
    var viewModels: [SelectTokenCellViewModel] = []
    var onTap: ((SelectTokenCellViewModel) -> Void)?
}
