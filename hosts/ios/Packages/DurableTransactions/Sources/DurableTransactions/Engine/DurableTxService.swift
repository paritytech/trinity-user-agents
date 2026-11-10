import AsyncExtensions
import BackgroundExecution
@preconcurrency import ExtrinsicService
import Foundation
import os
@preconcurrency import SDKLogger
import StructuredConcurrency
import SubstrateSdk

/// Owns one fact: the status of every submitted transaction, for every domain.
///
/// Whatever a domain locks for a transaction it keeps itself, written through the registration hook.
/// This service never reads chain state of its own beyond block bodies and the heads — anything
/// domain-shaped reaches it only through that domain's ``TxCompletionOracle``.
public protocol DurableTxServicing: Sendable {
    /// Builds, registers and submits several transactions as one operation: either all of them are
    /// recorded or none is. `onRegister` runs inside the registration transaction with the minted ids,
    /// so a domain's own rows commit together with the engine's. Returns once committed, which is before
    /// the bytes reach the wire: no extrinsic is ever in flight without a record.
    ///
    /// `policies` runs parallel to `requests`; a non-`nil` entry is the policy that builds that
    /// transaction again once an attempt is proven unable to land, instead of failing it.
    @discardableResult
    func submitTransactions(
        domain: TxDomainId,
        requests: [DurableTxRequest],
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy?],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId]

    /// Registers transactions that have not been built yet, one per policy, as one operation, joining a
    /// write the caller already opened.
    ///
    /// What they will consume is locked from the moment that write commits, so the payment's row and
    /// these become durable together. Synchronous because its caller already is — a transport writing the
    /// row that carries a payment runs inside its store's write block, which cannot suspend.
    @discardableResult
    func schedule(
        domain: TxDomainId,
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy],
        joining scope: any DurableTxRegistrationScope,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId]

    /// A stream of a transaction's status: the current value, then every change.
    func subscribeTransactionStatus(_ id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus>

    /// Every transaction registered under `groupId`, in registration order. Empty when nothing was ever
    /// registered under it.
    func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry]

    /// A stream of the transactions registered under `groupId`: the current set, then every change.
    func subscribeGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]>

    /// Starts a recovery pass without waiting for it. Never awaited by startup: a single unresolvable
    /// transaction must not hold the app for a mortality window.
    func startRecoveryPass()

    /// Runs one pass immediately, then a pass on every new head of every chain a registered domain lives
    /// on. Subsumes ``startRecoveryPass()``. Idempotent: a second call replaces the running loop, so a
    /// domain that registers its oracle after the loop started calls this again to have its chain watched.
    func start()

    /// Cancels the head-driven passes for every domain — the engine is shared. Safe to call when not
    /// started.
    func stop()
}

public extension DurableTxServicing {
    /// Submits transactions none of which is ever rebuilt — the shape every caller had before
    /// submission policies existed.
    @discardableResult
    func submitTransactions(
        domain: TxDomainId,
        requests: [DurableTxRequest],
        groupId: DurableTxGroupId?,
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        try await submitTransactions(
            domain: domain,
            requests: requests,
            groupId: groupId,
            policies: [],
            onRegister: onRegister
        )
    }
}

/// Orchestrates registration, submission tracking and the recovery pass, and exposes the queries.
///
/// Not an actor: most stored properties are `let` and every method suspends on its first statement, so
/// actor isolation would protect nothing. Registration is serialized by the store's transaction, and
/// ``DurableTxOwnershipSet`` carries its own lock. The head-driven task handle is the one mutable piece of
/// state, guarded by a lock.
public final class DurableTxService: DurableTxServicing, @unchecked Sendable {
    /// Kept to resolve a domain's chain at submission time; registration of oracles is
    /// ``DurableTxServices``' job, not this service's.
    private let oracles: TxCompletionOracleRegistry

    private let store: any DurableTxRepositoryProtocol
    private let registrar: DurableTxRegistrar
    private let tracker: DurableTxTracker
    private let launcher: DurableSubmissionLauncher
    private let executor: DurableSubmissionExecutor
    private let pass: DurableRecoveryPass
    private let chainFactory: any PinnedChainViewFactoryProtocol
    private let chainTools: any DurableChainToolsProviding
    private let logger: SDKLoggerProtocol?

    private let triggerTask = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)

    public init(
        store: any DurableTxRepositoryProtocol,
        registrar: DurableTxRegistrar,
        tracker: DurableTxTracker,
        launcher: DurableSubmissionLauncher,
        executor: DurableSubmissionExecutor,
        pass: DurableRecoveryPass,
        oracles: TxCompletionOracleRegistry,
        chainFactory: any PinnedChainViewFactoryProtocol,
        chainTools: any DurableChainToolsProviding,
        logger: SDKLoggerProtocol?
    ) {
        self.store = store
        self.registrar = registrar
        self.tracker = tracker
        self.launcher = launcher
        self.executor = executor
        self.pass = pass
        self.oracles = oracles
        self.chainFactory = chainFactory
        self.chainTools = chainTools
        self.logger = logger
    }

    /// Wires the engine over a store and the chain-bound seams, and hands back every entry point a
    /// domain needs: the service to submit through, the two registries to register with before it
    /// does, and the factory that turns declared transactions into signed ones.
    public static func make(
        store: any DurableTxRepositoryProtocol,
        chainViewFactory: any PinnedChainViewFactoryProtocol,
        chainTools: any DurableChainToolsProviding,
        backgroundExecutor: any BackgroundExecuting,
        logger: SDKLoggerProtocol?
    ) -> DurableTxServices {
        let owned = DurableTxOwnershipSet()
        let oracles = TxCompletionOracleRegistry()
        let policies = DurableSubmissionPolicyRegistry()
        let verdictWriter = DurableVerdictWriter(store: store, policies: policies, logger: logger)

        let pass = DurableRecoveryPass(
            store: store,
            chainFactory: chainViewFactory,
            owned: owned,
            oracles: oracles,
            verdictWriter: verdictWriter,
            logger: logger
        )

        let tracker = DurableTxTracker(
            store: store,
            chainFactory: chainViewFactory,
            owned: owned,
            verdictWriter: verdictWriter,
            backgroundExecutor: backgroundExecutor,
            logger: logger
        )

        let launcher = DurableSubmissionLauncher(
            store: store,
            tracker: tracker,
            owned: owned,
            chainTools: chainTools,
            onRecovery: { Task { await pass.run() } },
            logger: logger
        )

        let executor = DurableSubmissionExecutor(
            store: store,
            policies: policies,
            launcher: launcher,
            // Anything waiting to be built keeps recovery scheduled: the loop is what holds the
            // process awake long enough for a policy to finish waiting on the chain.
            onPendingSubmissions: { Task { await pass.run() } },
            backgroundExecutor: backgroundExecutor,
            logger: logger
        )

        let service = DurableTxService(
            store: store,
            registrar: DurableTxRegistrar(store: store, owned: owned, logger: logger),
            tracker: tracker,
            launcher: launcher,
            executor: executor,
            pass: pass,
            oracles: oracles,
            chainFactory: chainViewFactory,
            chainTools: chainTools,
            logger: logger
        )

        return DurableTxServices(
            txService: service,
            oracles: oracles,
            policies: policies,
            factory: DurableTxFactory(chainTools: chainTools, logger: logger)
        )
    }
}

// MARK: - DurableTxServicing

public extension DurableTxService {
    @discardableResult
    func submitTransactions(
        domain: TxDomainId,
        requests: [DurableTxRequest],
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy?],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        guard let chainId = oracles.chainId(for: domain) else {
            throw DurableTxError.unregisteredDomain(domain)
        }

        let operationFactory = try await chainTools.extrinsicOperationFactory(for: chainId)
        let submitter = try await chainTools.extrinsicSubmitter(for: chainId)

        // Build every extrinsic before registering any, so a build failure aborts before a single
        // extrinsic is broadcast. The batch then registers atomically; only then is each tracked.
        logger?.debug("Building \(requests.count) request(s) domain: \(domain) groupId: \(String(describing: groupId))")

        let builder = ExtrinsicBatchBuilder(operationFactory: operationFactory, logger: logger)
        let models = try await builder.build(requests)

        let registrations = try Self.registrations(
            domain: domain,
            groupId: groupId,
            models: models,
            policies: policies
        )
        let ids = try await registrar.register(registrations, onRegister: onRegister)

        logger?.debug("Registered transactions=\(ids.count) groupId=\(String(describing: groupId))")

        for (id, model) in zip(ids, models) {
            launcher.watch(id: id, model: model, chainId: chainId, submitter: submitter)
        }

        return ids
    }

    @discardableResult
    func schedule(
        domain: TxDomainId,
        groupId: DurableTxGroupId?,
        policies: [SubmissionPolicy],
        joining scope: any DurableTxRegistrationScope,
        onRegister: DurableTxRegistrationHook
    ) throws -> [DurableTxId] {
        let schedules = policies.map {
            DurableTxSchedule(domainId: domain, groupId: groupId, policy: $0)
        }

        let ids = try store.schedule(schedules, joining: scope, onRegister: onRegister)

        logger?.debug("Scheduled transactions=\(ids.count) inside a caller's transaction")

        // The executor reads committed rows only, so asking now is safe while the caller's transaction
        // is still open — it simply sees nothing until the caller commits.
        startRecoveryPass()

        return ids
    }

    func subscribeTransactionStatus(_ id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        store.subscribeStatus(id: id)
    }

    func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        try await store.getGroupEntries(domain: domain, groupId: groupId)
    }

    func subscribeGroupEntries(
        domain: TxDomainId,
        groupId: DurableTxGroupId
    ) -> AnyAsyncSequence<[DurableTxEntry]> {
        store.subscribeGroupEntries(domain: domain, groupId: groupId)
    }

    func startRecoveryPass() {
        Task { [pass, executor] in
            await executor.ensureStarted()
            await pass.run()
        }
    }

    func start() {
        let chainIds = oracles.chainIds

        let task = Task { [pass, chainFactory, executor] in
            // A relaunch reaches here with nothing having started the executor, and the loop would keep
            // a transaction waiting to be built alive without anyone building it.
            await executor.ensureStarted()

            await pass.run()

            // A pass on every newly finalized head and every new best head of every watched chain.
            // Finality repairs what a released watcher left pending; the best head advances several
            // blocks earlier, so `pendingSuccess` is picked up promptly. Passes coalesce, so frequent
            // best-head ticks do not stack up.
            await withTaskGroup(of: Void.self) { group in
                for chainId in chainIds {
                    group.addTask { await Self.runPass(on: chainFactory.finalizedHeads(chainId: chainId), pass: pass) }
                    group.addTask { await Self.runPass(on: chainFactory.bestHeads(chainId: chainId), pass: pass) }
                }
            }
        }
        let previous = triggerTask.withLock { current in
            let old = current
            current = task
            return old
        }
        previous?.cancel()
    }

    func stop() {
        let task = triggerTask.withLock { current in
            let old = current
            current = nil
            return old
        }
        task?.cancel()

        Task { [executor] in await executor.close() }
    }
}

// MARK: - Registration

private extension DurableTxService {
    /// Builds one registration per built extrinsic, pairing each with the policy that may build it
    /// again. The attempt itself — hash, checkpoint and window — is read off the built model by
    /// ``DurableTxAttempt/init(from:)``, which is the one place that derivation lives.
    static func registrations(
        domain: TxDomainId,
        groupId: DurableTxGroupId?,
        models: [ExtrinsicBuiltModel],
        policies: [SubmissionPolicy?]
    ) throws -> [DurableTxRegistration] {
        try models.enumerated().map { index, model in
            try DurableTxRegistration(
                domainId: domain,
                groupId: groupId,
                attempt: DurableTxAttempt(from: model),
                policy: index < policies.count ? policies[index] : nil
            )
        }
    }
}

// MARK: - Head-driven passes

private extension DurableTxService {
    /// Runs a pass on every head the stream yields. The factory's head streams are self-healing and never
    /// surface an error; a throw only means the stream ended, so there is nothing to do.
    static func runPass(on heads: AnyAsyncSequence<BlockNumber>, pass: DurableRecoveryPass) async {
        do {
            for try await _ in heads {
                guard !Task.isCancelled else { break }
                await pass.run()
            }
        } catch {}
    }
}
