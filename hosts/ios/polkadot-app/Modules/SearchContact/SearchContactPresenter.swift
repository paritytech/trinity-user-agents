import Foundation
import PolkadotUI
import UIKit
import DesignSystem
import SubstrateSdk

@MainActor
final class SearchContactPresenter {
    weak var view: SearchContactViewProtocol?
    let wireframe: SearchContactWireframeProtocol
    let interactor: SearchContactInteractorInputProtocol

    private var currentSearch = CurrentSearch(
        query: "",
        latestResult: .sections(AccountSearchSections(recent: [], contacts: [])),
        didReceiveWaiting: false
    )

    private var selection: [String: ContactSearchPayload] = [:]

    init(
        interactor: SearchContactInteractorInputProtocol,
        wireframe: SearchContactWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension SearchContactPresenter: SearchContactPresenterProtocol {
    func setup() {
        interactor.setup()
        provideViewModel(sections: AccountSearchSections(recent: [], contacts: []))
    }

    func search(username: String) {
        interactor.search(username: username)
    }

    func didSelectContact(identifier: String) {
        guard let contact = findContact(by: identifier) else {
            return
        }
        interactor.decide(on: contact)
    }
}

extension SearchContactPresenter: SearchContactInteractorOutputProtocol {
    func didReceive(searchState state: SearchContactSearchState, for query: String) {
        guard canApplySearchState(state, for: query) else {
            return
        }
        applySearchState(state, for: query)
    }

    func didReceive(error: any Error) {
        _ = wireframe.present(error: error, from: view)
    }

    func didReceive(resolution: ChatOpenModel) {
        wireframe.complete(from: view, with: resolution)
    }
}

private extension SearchContactPresenter {
    func canApplySearchState(_ state: SearchContactSearchState, for query: String) -> Bool {
        if case .started = state {
            return true
        }
        return query.isEmpty || currentSearch.query == query
    }

    func applySearchState(_ state: SearchContactSearchState, for query: String) {
        currentSearch = makeCurrentSearch(applying: state, for: query)

        switch state {
        case let .result(.sections(sections)):
            selection = Dictionary(
                (sections.recent + sections.contacts + sections.global.rows)
                    .map { ($0.payload.accountId.toHex(), $0.payload) },
                uniquingKeysWith: { first, _ in first }
            )
            provideViewModel(sections: sections)
        case .result(.error):
            selection = [:]
            provideViewModel(sections: AccountSearchSections(recent: [], contacts: []))
        case .started,
             .waiting:
            provideStatus()
        }
    }

    /// The `.waiting` signal only records that the loader is due; whether it shows is decided
    /// from the latest phase, which may land either before or after the signal.
    func makeCurrentSearch(applying state: SearchContactSearchState, for query: String) -> CurrentSearch {
        switch state {
        case .started:
            CurrentSearch(query: query, latestResult: nil, didReceiveWaiting: false)
        case .waiting:
            CurrentSearch(
                query: query,
                latestResult: currentSearch.latestResult,
                didReceiveWaiting: true
            )
        case let .result(result):
            CurrentSearch(
                query: query,
                latestResult: result,
                didReceiveWaiting: currentSearch.didReceiveWaiting
            )
        }
    }

    func makeStatus() -> SearchContactResultsView.StatusViewModel {
        SearchContactResultsView.StatusViewModel(
            message: makeStatusMessage(),
            showsLoader: currentSearch.showsLoader
        )
    }

    /// Shown instead of the rows once the search settles: the failure reason, or the
    /// no-recents hint when the field is empty. A failed global lookup stays silent while
    /// recent or contact rows are on screen, since those are still usable.
    func makeStatusMessage() -> NSAttributedString? {
        guard !currentSearch.isSearching else {
            return nil
        }

        let query = currentSearch.query
        let allEmpty = selection.isEmpty

        if currentSearch.globalFailed {
            return allEmpty ? makeCenteredMessage(String(localized: .accountSearchGlobalFailed)) : nil
        } else if currentSearch.queryFailed || (!query.isEmpty && allEmpty) {
            return makeCenteredMessage(String(localized: .searchContactNoSuchUsername(username: query)))
        } else if allEmpty, query.isEmpty {
            return makeCenteredMessage(String(localized: .searchContactNoRecentSearches))
        } else {
            return nil
        }
    }

    func makeCenteredMessage(_ text: String) -> NSAttributedString {
        var attributes = LabelStyle.title16SemiBold().attributes(for: .center)
        attributes[.foregroundColor] = UIColor.fgSecondary
        return NSAttributedString(string: text, attributes: attributes)
    }

    func provideStatus() {
        view?.didReceive(status: makeStatus())
    }

    func provideViewModel(sections: AccountSearchSections<ContactSearchPayload, ContactSearchPayload>) {
        let viewModel = SearchContactResultsView.ViewModel(
            sections: buildViewSections(from: sections),
            status: makeStatus()
        )

        view?.didReceive(viewModel: viewModel)
    }

    func buildViewSections(
        from sections: AccountSearchSections<ContactSearchPayload, ContactSearchPayload>
    ) -> [SearchContactResultsView.ViewModel.Section] {
        [
            makeViewSection(
                id: "recent",
                title: String(localized: .searchContactRecentChats),
                rows: sections.recent
            ),
            makeViewSection(
                id: "contacts",
                title: String(localized: .transactionSearchMyContacts),
                rows: sections.contacts
            ),
            makeViewSection(
                id: "global",
                title: String(localized: .transactionSearchAllUsers),
                rows: sections.global.rows
            )
        ].compactMap { $0 }
    }

    func makeViewSection(
        id: String,
        title: String,
        rows: [SearchRow<ContactSearchPayload>]
    ) -> SearchContactResultsView.ViewModel.Section? {
        guard !rows.isEmpty else { return nil }

        return SearchContactResultsView.ViewModel.Section(
            id: id,
            title: title,
            rows: rows.map { row in
                IdentifiableContentConfiguration(
                    id: row.payload.accountId.toHex(),
                    configuration: makeListConfiguration(for: row.payload)
                )
            }
        )
    }

    func makeListConfiguration(for payload: ContactSearchPayload) -> SearchContactListConfiguration {
        let prefix = String(payload.username.prefix(1))
        let avatarViewModel = AvatarViewModel.colored(
            text: prefix,
            colorSeed: payload.accountId.toHex()
        )
        return SearchContactListConfiguration(
            userName: payload.username,
            avatarViewModel: avatarViewModel
        )
    }

    func findContact(by identifier: String) -> ContactSearchPayload? {
        selection[identifier]
    }

    struct CurrentSearch {
        let query: String
        /// The latest phase received for this query, or nil while none has arrived yet.
        let latestResult: SearchContactSearchResult?
        let didReceiveWaiting: Bool

        var queryFailed: Bool {
            guard case .error = latestResult else {
                return false
            }
            return true
        }

        var globalFailed: Bool {
            sections?.global.hasFailed == true
        }

        /// A phase whose global lookup is still pending counts as searching, so the no-results
        /// message does not flash over an empty screen before the global rows land.
        var isSearching: Bool {
            guard latestResult != nil else {
                return true
            }
            return sections?.global.isPending == true
        }

        var showsLoader: Bool {
            guard didReceiveWaiting else {
                return false
            }
            guard latestResult != nil else {
                return true
            }
            guard let sections, sections.global.isPending else {
                return false
            }
            return sections.recent.isEmpty && sections.contacts.isEmpty
        }

        /// The latest phase's sections, or nil when no phase has arrived or the search failed outright.
        private var sections: AccountSearchSections<ContactSearchPayload, ContactSearchPayload>? {
            guard case let .sections(sections) = latestResult else {
                return nil
            }
            return sections
        }
    }
}
