import Foundation
import Foundation_iOS
import ChainRegistry
import SubstrateSdkExt

@MainActor
final class SearchAccountPresenter {
    // MARK: Properties

    weak var view: SearchAccountViewProtocol?
    let wireframe: SearchAccountWireframeProtocol
    let interactor: SearchAccountInteractorInputProtocol
    private let chainAsset: ChainAsset
    private var addressInputViewModel = InputViewModel.createAccountInputViewModel(for: "")
    private let recipientViewModelFactory: RecipientViewModelFactoryProtocol
    private var currentSearch = CurrentSearch(query: "", latestResult: nil, didReceiveWaiting: false)

    init(
        interactor: SearchAccountInteractorInputProtocol,
        wireframe: SearchAccountWireframeProtocol,
        recipientViewModelFactory: RecipientViewModelFactoryProtocol,
        chainAsset: ChainAsset
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.recipientViewModelFactory = recipientViewModelFactory
        self.chainAsset = chainAsset
    }

    private func provideAddressInputViewModel(_ accountType: SearchAccountViewModel.AccountType? = nil) {
        guard let view else { return }

        view.didReceive(
            SearchAccountViewModel(
                inputViewModel: SearchAccountViewModel.InputModel(
                    inputViewModel: addressInputViewModel,
                    selectedAccount: accountType
                ),
                content: view.viewModel.content
            )
        )
    }

    private func handleAccountSelection(_ accountType: SearchAccountViewModel.AccountType) {
        addressInputViewModel = InputViewModel.createAccountInputViewModel(for: accountType.title)
        provideAddressInputViewModel(accountType)
    }

    private func updateViewModel(content: SearchAccountViewModel.Content) {
        let viewModel = SearchAccountViewModel(
            inputViewModel: SearchAccountViewModel.InputModel(
                inputViewModel: addressInputViewModel
            ),
            content: content
        )
        view?.applyData(viewModel)
    }

    private static func mapToAccountType(
        _ contact: SearchAccountResult.Contact
    ) -> SearchAccountViewModel.AccountType {
        guard let username = contact.username, !username.isEmpty else {
            return .accountAddress(contact.address)
        }
        return .username(username, contact.address)
    }
}

// MARK: - SearchAccountPresenterProtocol

extension SearchAccountPresenter: SearchAccountPresenterProtocol {
    func viewDidLoad() {
        interactor.setup()
        provideAddressInputViewModel()
    }

    func scanQRCode() {
        wireframe.showQRScan(from: view)
    }

    func searchAccount(_ account: String?) {
        let query = account?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        currentSearch = CurrentSearch(query: query, latestResult: nil, didReceiveWaiting: false)
        interactor.searchAccount(for: account)
    }

    func selectAccount(_ cellType: SearchAccountViewController.Cell) {
        switch cellType {
        case let .globalContact(accountType):
            interactor.resolveChat(for: accountType.accountAddress)
        case .account,
             .recentContact:
            handleAccountSelection(cellType.accountType)
            guard let recipient = try? RecipientModel(accountType: cellType.accountType) else { return }
            wireframe.showTransfer(from: view, recipient: recipient, chainAsset: chainAsset)
        }
    }

    func didEndEditingInput(_ input: String?) {
        guard
            let inputText = input?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !inputText.isEmpty
        else {
            return
        }

        let isValidAddress = (try? inputText.toAccountId(using: chainAsset.chain.chainFormat)) != nil

        let accountType: SearchAccountViewModel.AccountType = isValidAddress
            ? .accountAddress(inputText)
            : .username(inputText, inputText)

        provideAddressInputViewModel(accountType)
    }
}

// MARK: - SearchAccountInteractorOutputProtocol

extension SearchAccountPresenter: SearchAccountInteractorOutputProtocol {
    func didReceive(searchState: SearchAccountSearchState) {
        currentSearch = currentSearch.applying(searchState)

        switch searchState {
        case .started,
             .waiting:
            provideStatus()
        case let .result(result):
            updateViewModel(
                content: SearchAccountViewModel.Content(
                    recent: recipientViewModelFactory.createRecentContacts(from: result.recent),
                    contacts: result.contacts.map(Self.mapToAccountType),
                    global: result.global.rows.map(Self.mapToAccountType)
                )
            )
            provideStatus()
        }
    }

    func didResolveChat(_ model: ChatOpenModel) {
        wireframe.showChat(model)
    }

    func didReceiveSearchError(message: String?) {
        wireframe.present(
            message: message,
            title: String(localized: .Common.error),
            closeAction: String(localized: .Common.close),
            from: view
        )
    }
}

// MARK: - Private

private extension SearchAccountPresenter {
    func provideStatus() {
        view?.didReceive(
            status: SearchAccountViewModel.Status(
                message: makeStatusMessage(),
                showsLoader: currentSearch.showsLoader
            )
        )
    }

    /// Shown instead of the rows once the search settles. A failed global lookup stays silent
    /// while recent or contact rows are on screen, since those are still usable.
    func makeStatusMessage() -> String? {
        guard !currentSearch.isSearching, currentSearch.isEmpty else {
            return nil
        }

        if currentSearch.globalFailed {
            return String(localized: .accountSearchGlobalFailed)
        } else if !currentSearch.query.isEmpty {
            return String(localized: .searchContactNoSuchUsername(username: currentSearch.query))
        } else {
            return nil
        }
    }

    struct CurrentSearch {
        let query: String
        /// The latest phase received for this query, or nil while none has arrived yet.
        let latestResult: SearchAccountResult?
        let didReceiveWaiting: Bool

        var isEmpty: Bool {
            guard let latestResult else {
                return true
            }
            return latestResult.recent.isEmpty && latestResult.contacts.isEmpty && latestResult.global.rows.isEmpty
        }

        var globalFailed: Bool {
            latestResult?.global.hasFailed == true
        }

        /// A phase whose global lookup is still pending counts as searching, so the no-results
        /// message does not flash over an empty screen before the global rows land.
        var isSearching: Bool {
            guard let latestResult else {
                return true
            }
            return latestResult.global.isPending
        }

        var showsLoader: Bool {
            guard didReceiveWaiting else {
                return false
            }
            guard let latestResult else {
                return true
            }
            guard latestResult.global.isPending else {
                return false
            }
            return latestResult.recent.isEmpty && latestResult.contacts.isEmpty
        }

        /// The `.waiting` signal only records that the loader is due; whether it shows is decided
        /// from the latest phase, which may land either before or after the signal.
        func applying(_ state: SearchAccountSearchState) -> CurrentSearch {
            switch state {
            case .started:
                CurrentSearch(query: query, latestResult: nil, didReceiveWaiting: false)
            case .waiting:
                CurrentSearch(query: query, latestResult: latestResult, didReceiveWaiting: true)
            case let .result(result):
                CurrentSearch(query: query, latestResult: result, didReceiveWaiting: didReceiveWaiting)
            }
        }
    }
}
