import Foundation
@preconcurrency import SDKLogger
import SubstrateSdk

/// One bounded sweep over the live transactions that submission tracking does not own.
///
/// Every domain read a pass needs is issued once, up front, by that domain's oracle. The scope it returns
/// does not suspend, so rule evaluation cannot go back to the chain per transaction. Rules run entirely
/// outside any database transaction — the body search can span a whole mortality window — and the write
/// is a compare-and-set against the status the rules were evaluated from, so a status that moved
/// underneath costs that transaction a pass and nothing else.
///
/// Never awaited by startup: a single unresolvable transaction must not hold the app for a mortality
/// window.
public actor DurableRecoveryPass {
    private let store: any DurableTxRepositoryProtocol
    private let chainFactory: any PinnedChainViewFactoryProtocol
    private let owned: DurableTxOwnershipSet
    private let oracles: TxCompletionOracleRegistry
    private let verdictWriter: DurableVerdictWriter
    private let ladder: CompletionLadder
    private let logger: SDKLoggerProtocol?

    private var isRunning = false
    private var rerunRequested = false

    private static let maxCoalescedPasses = 2

    public init(
        store: any DurableTxRepositoryProtocol,
        chainFactory: any PinnedChainViewFactoryProtocol,
        owned: DurableTxOwnershipSet,
        oracles: TxCompletionOracleRegistry,
        verdictWriter: DurableVerdictWriter,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.chainFactory = chainFactory
        self.owned = owned
        self.oracles = oracles
        self.verdictWriter = verdictWriter
        ladder = CompletionLadder(logger: logger)
        self.logger = logger
    }

    /// Runs one pass. At most one runs at a time; a request arriving during a pass causes one additional
    /// pass afterwards rather than being dropped, capped to prevent unbounded spinning.
    public func run() async {
        guard !isRunning else {
            rerunRequested = true
            return
        }
        isRunning = true
        defer { isRunning = false }

        var iterations = 0
        repeat {
            rerunRequested = false
            do {
                try await performPass()
            } catch {
                logger?.error("Recovery pass failed: \(error)")
            }
            iterations += 1
        } while rerunRequested && iterations < Self.maxCoalescedPasses
    }
}

// MARK: - Pass

private extension DurableRecoveryPass {
    func performPass() async throws {
        let domains = try await decidableDomains()

        logger?.debug("Recovery pass: \(domains.count) domains to process")

        // Nothing to decide means nothing to read, so the pass ends before pinning a view — which would
        // be a chain read with nothing to read it for.
        guard !domains.isEmpty else { return }

        // One view per distinct chain, not per domain: two domains on the same chain read the same heads,
        // and pinning is a chain read worth paying for once. A domain with no registered oracle has no
        // chain to pin, so it is skipped rather than guessed at.
        var byChain: [ChainId: [TxDomainId]] = [:]
        for domain in domains {
            guard let chainId = oracles.chainId(for: domain) else {
                logger?.warning("Recovery pass skipped domain=\(domain) reason=no-registered-oracle")
                continue
            }
            byChain[chainId, default: []].append(domain)
        }

        for (chainId, chainDomains) in byChain {
            await runChainPass(chainId: chainId, domains: chainDomains)
        }
    }

    /// Domains with at least one live transaction no submission owns, in first-seen order.
    func decidableDomains() async throws -> [TxDomainId] {
        var seen: Set<TxDomainId> = []
        return try await store.getAllEntries()
            .filter { $0.status.awaitsVerdict && !owned.isOwned($0.id) }
            .map(\.domainId)
            .filter { seen.insert($0).inserted }
    }

    /// A chain that cannot be read leaves its domains for the next pass; the other chains are still
    /// worked, so one unreachable chain does not hold the rest up.
    func runChainPass(chainId: ChainId, domains: [TxDomainId]) async {
        let view: any PinnedChainViewProtocol
        do {
            view = try await chainFactory.pin(chainId: chainId)
        } catch {
            logger?.warning("Recovery pass chain=\(chainId) pin-failed error=\(error)")
            return
        }

        logger?.debug(
            "Recovery pass chain=\(chainId) domains=\(domains.count) "
                + "f=\(view.finalizedHead.number) b=\(view.bestHead.number)"
        )

        for domain in domains {
            do {
                try await runDomainPass(domain, view: view)
            } catch {
                logger?.warning("Recovery pass domain=\(domain) failed error=\(error)")
            }
        }
    }

    /// Two rounds, deliberately. A transaction promoted in the first round is exactly the evidence its
    /// predecessor needs, and a domain reads statuses when it opens a pass — so the second round sees what
    /// the first wrote. Beyond two the loop would have to reach a fixpoint, which passes are cheap enough
    /// not to need: the next head runs another one.
    func runDomainPass(_ domain: TxDomainId, view: any PinnedChainViewProtocol) async throws {
        let first = try await evaluateRound(domain, view: view)
        guard first > 0 else { return }

        let propagated = try await evaluateRound(domain, view: view)
        logger?.debug("Recovery pass end domain=\(domain) written=\(first) propagated=\(propagated)")
    }

    /// Returns how many transactions this round wrote.
    func evaluateRound(_ domain: TxDomainId, view: any PinnedChainViewProtocol) async throws -> Int {
        guard let oracle = oracles.oracle(for: domain) else { return 0 }

        let all = try await store.getAllEntries(domain: domain)
        let decidable = all.filter { $0.status.awaitsVerdict && !owned.isOwned($0.id) }
        guard !decidable.isEmpty else { return 0 }

        logger?.debug("Decidable for domain=\(domain) count=\(decidable.count)")

        let scope = try await oracle.openPass(
            transactions: decidable,
            ledger: SnapshotLedgerView(transactions: all),
            view: view
        )

        // Batched with the domain's own reads rather than per transaction inside Rule 0.
        let canonical = await recordedCanonicality(decidable, view: view)

        var wrote = 0
        for transaction in decidable {
            let outcome = await ladder.evaluate(
                transaction,
                scope: scope,
                view: view,
                recordedStillCanonical: canonical[transaction.id]
            )
            if case let .decided(verdict) = outcome, await write(transaction, verdict) {
                wrote += 1
            }
        }

        logger?.debug(
            "Recovery pass round domain=\(domain) transactions=\(all.count) "
                + "decidable=\(decidable.count) written=\(wrote)"
        )
        return wrote
    }

    /// Whether each recorded block is still canonical, one read per distinct block height.
    ///
    /// A missing entry is a read that failed, which leaves Rule 0 undecided rather than discarding a
    /// record on a transport error. A chain shorter than the record does not have the block, so the
    /// record is stale.
    func recordedCanonicality(
        _ transactions: [DurableTxEntry],
        view: any PinnedChainViewProtocol
    ) async -> [DurableTxId: Bool] {
        let recorded = transactions
            .compactMap { transaction in transaction.successDetectedAt.map { (transaction.id, $0) } }
        guard !recorded.isEmpty else { return [:] }

        var hashesByHeight: [UInt32: ReadResult<Data>] = [:]
        for height in Set(recorded.map(\.1.number)) {
            hashesByHeight[height] = await view.blockHash(at: height)
        }

        var canonical: [DurableTxId: Bool] = [:]
        for (id, block) in recorded {
            switch hashesByHeight[block.number] {
            case let .present(hash): canonical[id] = hash == block.hash
            case .absent: canonical[id] = false
            case .failedRead,
                 .none: break
            }
        }
        return canonical
    }

    /// Compare-and-set against the status the verdict was formed from. Skips a write that would change
    /// nothing.
    func write(_ transaction: DurableTxEntry, _ verdict: Verdict) async -> Bool {
        guard verdict.status != transaction.status || verdict.successDetectedAt != transaction.successDetectedAt else {
            return false
        }
        do {
            return try await verdictWriter.write(transaction, verdict)
        } catch {
            logger?.error("Verdict write failed for \(transaction.id): \(error)")
            return false
        }
    }
}
