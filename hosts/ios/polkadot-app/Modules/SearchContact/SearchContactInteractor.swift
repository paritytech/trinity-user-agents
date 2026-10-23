import Foundation
import AsyncExtensions
import os

final class SearchContactInteractor {
    weak var presenter: SearchContactInteractorOutputProtocol?

    private let accountSearching: any AccountSearching<ContactSearchPayload, ContactSearchPayload>
    private let searchRunner = SearchRunner()
    private let stateLock: OSAllocatedUnfairLock<State>

    init(accountSearching: any AccountSearching<ContactSearchPayload, ContactSearchPayload>) {
        self.accountSearching = accountSearching
        stateLock = OSAllocatedUnfairLock(initialState: State())
    }

    deinit {
        replaceSearchTask(with: nil)
        replaceSourcesChangedTask(with: nil)
    }
}

extension SearchContactInteractor: SearchContactInteractorInputProtocol {
    func setup() {
        accountSearching.setup()
        subscribeToSourcesChanged()
        loadIdleState()
    }

    func search(username: String) {
        guard !username.isEmpty else {
            loadIdleState()
            return
        }

        stateLock.withLock { $0.currentQuery = username }

        let task = Task { [weak presenter, searchRunner, accountSearching] in
            let stateStream = searchRunner.run({
                accountSearching.searchPhases(query: username)
                    .mapToResult()
                    .map(\.searchResult)
            }, hasContent: \.hasContent)
            for await state in stateStream {
                guard !Task.isCancelled else { return }
                await presenter?.didReceive(searchState: state, for: username)
            }
        }

        replaceSearchTask(with: task)
    }

    /// The chat product opens the chat with the account, a contact or not (HX2).
    func decide(on payload: ContactSearchPayload) {
        let accountId =
            switch payload {
            case let .local(contact): contact.accountId
            case let .remote(contact): contact.accountId
            }

        Task { [weak self] in
            await self?.presenter?.didReceive(resolution: .existingChat(.person(accountId)))
        }
    }
}

private extension SearchContactInteractor {
    struct State {
        var currentQuery: String?
        var searchTask: Task<Void, Never>?
        var sourcesChangedTask: Task<Void, Never>?
    }

    func subscribeToSourcesChanged() {
        let task = Task { [weak self] in
            guard let self else { return }
            do {
                for try await _ in accountSearching.sourcesChanged() {
                    guard !Task.isCancelled else { return }
                    let query = stateLock.withLock { $0.currentQuery }
                    if let query, !query.isEmpty {
                        search(username: query)
                    } else {
                        loadIdleState()
                    }
                }
            } catch {
                // Subscription ended
            }
        }

        replaceSourcesChangedTask(with: task)
    }

    /// Swaps the stored handle under the lock and cancels the displaced task outside it,
    /// so two concurrent callers cannot both install a task and leak one uncancelled.
    func replaceSearchTask(with task: Task<Void, Never>?) {
        let previous = stateLock.withLock { state in
            let previous = state.searchTask
            state.searchTask = task
            return previous
        }

        previous?.cancel()
    }

    func replaceSourcesChangedTask(with task: Task<Void, Never>?) {
        let previous = stateLock.withLock { state in
            let previous = state.sourcesChangedTask
            state.sourcesChangedTask = task
            return previous
        }

        previous?.cancel()
    }

    func loadIdleState() {
        stateLock.withLock { $0.currentQuery = "" }

        let task = Task { [weak self, weak presenter] in
            guard let self else { return }
            do {
                for try await sections in accountSearching.searchPhases(query: nil) {
                    guard !Task.isCancelled else { return }
                    await presenter?.didReceive(searchState: .result(.sections(sections)), for: "")
                }
            } catch {
                guard !Task.isCancelled else { return }
                await presenter?.didReceive(error: error)
            }
        }

        replaceSearchTask(with: task)
    }
}

private extension Result where
    Success == AccountSearchSections<ContactSearchPayload, ContactSearchPayload>,
    Failure == Error {
    var searchResult: SearchContactSearchResult {
        switch self {
        case let .success(sections):
            .sections(sections)
        case let .failure(error):
            .error(error)
        }
    }
}

private extension SearchContactSearchResult {
    var hasContent: Bool {
        guard case let .sections(sections) = self else { return false }

        return sections.hasContent
    }
}
