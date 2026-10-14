import PolkadotUI
import UIKitExt

protocol SearchContactViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: SearchContactResultsView.ViewModel)
    func didReceive(status: SearchContactResultsView.StatusViewModel)
}

@MainActor
protocol SearchContactPresenterProtocol: AnyObject {
    func setup()
    func search(username: String)
    func didSelectContact(identifier: String)
}

protocol SearchContactInteractorInputProtocol: AnyObject {
    func setup()
    func search(username: String)
    func decide(on payload: ContactSearchPayload)
}

@MainActor
protocol SearchContactInteractorOutputProtocol: AnyObject {
    func didReceive(searchState: SearchContactSearchState, for query: String)
    func didReceive(error: Error)
    func didReceive(resolution: ChatOpenModel)
}

@MainActor
protocol SearchContactWireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {
    func complete(from view: SearchContactViewProtocol?, with model: ChatOpenModel)
}
