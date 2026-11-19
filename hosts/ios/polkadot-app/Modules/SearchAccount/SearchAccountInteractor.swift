import SubstrateSdk
import ChainRegistry
import Foundation_iOS
import os
import AsyncExtensions
import Products

private typealias SearchAccountSections = AccountSearchSections<RecentContactModelWithUsername, ContactSearchPayload>

final class SearchAccountInteractor {
    // MARK: Properties

    weak var presenter: SearchAccountInteractorOutputProtocol?

    private let accountSearching: any AccountSearching<
        RecentContactModelWithUsername,
        ContactSearchPayload
    >
    private let chatOpenResolver: ChatOpenModelResolving
    private let searchRunner = SearchRunner()
    private let logger: LoggerProtocol
    private let chainAsset: ChainAsset
    private let fundingDomainProvider: FundingDomainProviding
    private let stateLock: OSAllocatedUnfairLock<State>

    private static let maximumPrefixCount = 32

    // MARK: Initial methods

    init(
        accountSearching: any AccountSearching<
            RecentContactModelWithUsername,
            ContactSearchPayload
        >,
        chatOpenResolver: ChatOpenModelResolving,
        chainAsset: ChainAsset,
        logger: LoggerProtocol,
        fundingDomainProvider: FundingDomainProviding
    ) {
        self.accountSearching = accountSearching
        self.chatOpenResolver = chatOpenResolver
        self.chainAsset = chainAsset
        self.logger = logger
        self.fundingDomainProvider = fundingDomainProvider
        stateLock = OSAllocatedUnfairLock(initialState: State())
    }

    deinit {
        replaceSearchTask(with: nil)
        replaceSetupTask(with: nil)
        replaceWithdrawTask(with: nil)
    }
}

// MARK: - SearchAccountInteractorInputProtocol

extension SearchAccountInteractor: SearchAccountInteractorInputProtocol {
    func setup() {
        accountSearching.setup()
        subscribeToSourcesChanged()
        loadIdleState()
    }

    func searchAccount(for input: String?) {
        let trimmed = input?.trimmingCharacters(in: .whitespacesAndNewlines)

        guard let query = trimmed, !query.isEmpty else {
            stateLock.withLock { $0.query = nil }
            loadIdleState()
            return
        }

        stateLock.withLock { $0.query = query }

        guard isSearchable(query) else {
            emit(.result(SearchAccountResult(recent: [], contacts: [])), for: query)
            return
        }

        performSearch(query: query)
    }

    func resolveChat(for address: AccountAddress) {
        guard let accountId = try? address.toAccountId(using: chainAsset.chain.chainFormat) else { return }

        let contact: Chat.RemoteContact? = stateLock.withLock { state in
            state.globalContacts[accountId]
        }

        guard let contact else { return }

        Task { [weak self, chatOpenResolver] in
            do {
                let model = try await chatOpenResolver.resolveOpenModel(for: contact)
                await self?.presenter?.didResolveChat(model)
            } catch {
                await self?.presenter?.didReceiveSearchError(message: error.localizedDescription)
            }
        }
    }

    func openWithdrawProduct() {
        let task = Task { [weak presenter, fundingDomainProvider] in
            do {
                let page = try await RampAction.withdraw.resolvePage(using: fundingDomainProvider)
                await presenter?.didResolveWithdrawProduct(.success(page))
            } catch {
                await presenter?.didResolveWithdrawProduct(.failure(error))
            }
        }

        replaceWithdrawTask(with: task)
    }
}

// MARK: - Private

private extension SearchAccountInteractor {
    struct State {
        var query: String?
        var globalContacts: [AccountId: Chat.RemoteContact] = [:]
        var searchTask: Task<Void, Never>?
        var setupTask: Task<Void, Never>?
        var withdrawTask: Task<Void, Never>?
    }

    func isSearchable(_ query: String) -> Bool {
        let isValidAddress = (try? query.toAccountId(using: chainAsset.chain.chainFormat)) != nil

        return isValidAddress || query.count <= Self.maximumPrefixCount
    }

    func subscribeToSourcesChanged() {
        let task = Task { [weak self] in
            guard let self else { return }
            do {
                for try await _ in accountSearching.sourcesChanged() {
                    guard !Task.isCancelled else { return }

                    if let query = stateLock.withLock({ $0.query }) {
                        guard isSearchable(query) else { continue }
                        performSearch(query: query)
                    } else {
                        loadIdleState()
                    }
                }
            } catch {
                // Subscription ended
            }
        }

        replaceSetupTask(with: task)
    }

    func loadIdleState() {
        let task = Task { [weak self] in
            guard let self else { return }

            do {
                for try await sections in accountSearching.searchPhases(query: nil) {
                    guard !Task.isCancelled else { return }
                    emit(.result(makeResult(from: sections)), for: nil)
                }
            } catch {
                logger.error("Load idle state failed: \(error)")
            }
        }

        replaceSearchTask(with: task)
    }

    func performSearch(query: String) {
        let task = Task { [weak self, searchRunner, accountSearching] in
            guard let self else { return }

            let stream = searchRunner.run(
                { accountSearching.searchPhases(query: query).mapToResult() },
                hasContent: \.hasContent
            )

            for await state in stream {
                guard !Task.isCancelled else { return }

                await handle(state, for: query)
            }
        }

        replaceSearchTask(with: task)
    }

    /// A failure empties the list so that stale rows do not outlive a failed search.
    func handle(
        _ state: SearchRunner.State<Result<SearchAccountSections, Error>>,
        for query: String
    ) async {
        switch state {
        case .started:
            emit(.started, for: query)
        case .waiting:
            emit(.waiting, for: query)
        case let .result(.success(sections)):
            emit(.result(makeResult(from: sections)), for: query)
        case let .result(.failure(error)):
            logger.error("Search failed: \(error)")
            await presenter?.didReceiveSearchError(message: error.localizedDescription)
            emit(.result(SearchAccountResult(recent: [], contacts: [])), for: query)
        }
    }

    func makeResult(from sections: SearchAccountSections) -> SearchAccountResult {
        let globalContacts = sections.global.rows.compactMap { row -> (AccountId, Chat.RemoteContact)? in
            switch row.payload {
            case let .remote(contact): (row.accountId, contact)
            case .local: nil
            }
        }

        stateLock.withLock { state in
            state.globalContacts = Dictionary(uniqueKeysWithValues: globalContacts)
        }

        return SearchAccountResult(
            recent: sections.recent.map(\.payload),
            contacts: mapToContacts(sections.contacts),
            global: sections.global.map(mapToContacts)
        )
    }

    func mapToContacts(_ rows: [SearchRow<ContactSearchPayload>]) -> [SearchAccountResult.Contact] {
        rows.compactMap { row in
            guard let address = try? row.accountId.toAddress(using: chainAsset.chain.chainFormat) else {
                return nil
            }
            return SearchAccountResult.Contact(username: row.username?.value, address: address)
        }
    }

    func emit(_ state: SearchAccountSearchState, for query: String?) {
        let isCurrent = stateLock.withLock { $0.query == query }

        guard isCurrent else { return }

        Task { [weak presenter] in
            await presenter?.didReceive(searchState: state)
        }
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

    func replaceSetupTask(with task: Task<Void, Never>?) {
        let previous = stateLock.withLock { state in
            let previous = state.setupTask
            state.setupTask = task
            return previous
        }

        previous?.cancel()
    }

    func replaceWithdrawTask(with task: Task<Void, Never>?) {
        let previous = stateLock.withLock { state in
            let previous = state.withdrawTask
            state.withdrawTask = task
            return previous
        }

        previous?.cancel()
    }
}

private extension Result where Success == SearchAccountSections, Failure == Error {
    var hasContent: Bool {
        guard case let .success(sections) = self else { return false }

        return sections.hasContent
    }
}
