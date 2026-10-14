import Foundation
import BandersnatchApi
import Foundation_iOS
import SubstrateSdk
import SubstrateStorageSubscription
import Individuality
import ExtrinsicService
import StructuredConcurrency
import Operation_iOS
import BigInt
import SDKLogger
import KeyDerivation
import AsyncExtensions
import CommonService
import OperationExt
import os

/// A service that monitors local vouchers and synchronizes their on-chain status.
///
/// Tracks every voucher the durability layer still cares about (`shouldTrackOnchain` — not spent, not
/// mint-failed), not just those outside a recycler: a ring keeps filling after a voucher lands in it, and
/// the member count is what the strategies read to decide when it may be spent. Two-phase per batch, held
/// together entirely by stream operators rather than in-memory
/// reconciliation:
/// 1. **Members**: subscribe `Members[collection][voucherPubKey]` for every voucher and `scan` the
///    per-key deltas into a complete positions snapshot.
/// 2. **RingKeysStatus + RecyclersUnloadedCount**: `flatMapLatest` re-derives the ring-state
///    subscription from the current positions and `scan`s its deltas into complete snapshots, then joins
///    them into the resolved location — onboarding, or in-recycler(ringIndex, ringMembers) — together
///    with the ring's fungibility, which is a function of those two readings and the ring capacity.
public final class VoucherLocationService: BaseSyncService {
    private let instanceId: CoinageInstanceId
    private let voucherRepository: AnyDataProviderRepository<Voucher>
    private let databaseFactory: any DatabaseDependencyFactoring
    private let connection: JSONRPCEngine
    private let runtimeService: RuntimeCodingServiceProtocol
    private let ringCapacityProvider: any RingCapacityProviding

    private var localVouchersMonitoringTask: Task<Void, Error>?
    private var voucherStatusSubscriptionTask: Task<Void, Error>?

    public init(
        instanceId: CoinageInstanceId,
        voucherRepository: AnyDataProviderRepository<Voucher>,
        databaseFactory: any DatabaseDependencyFactoring,
        connection: JSONRPCEngine,
        runtimeService: RuntimeCodingServiceProtocol,
        ringCapacityProvider: any RingCapacityProviding,
        logger: any SDKLoggerProtocol
    ) {
        self.instanceId = instanceId
        self.voucherRepository = voucherRepository
        self.databaseFactory = databaseFactory
        self.connection = connection
        self.runtimeService = runtimeService
        self.ringCapacityProvider = ringCapacityProvider
        super.init(logger: logger)
    }

    deinit {
        stopSyncUp()
    }

    /// Begins monitoring the local database for vouchers worth tracking on chain (`shouldTrackOnchain`),
    /// resolving their location (onboarding → in-recycler) and refreshing the ring member count.
    override public func performSyncUp() {
        localVouchersMonitoringTask = Task { [weak self] in
            guard let self else { return }

            let stream = databaseFactory.makeTrackedVoucherSnapshotStream()
                .map { trackedVouchers in
                    trackedVouchers.filter(\.shouldTrackOnchain).map(\.voucher)
                }
                // A restore clears enteredAt; resubscribe so its next confirmation starts a new timer.
                .removeDuplicates { previous, current in
                    Dictionary(uniqueKeysWithValues: previous.map { ($0.publicKey, $0.recycler?.enteredAt) })
                        == Dictionary(uniqueKeysWithValues: current.map { ($0.publicKey, $0.recycler?.enteredAt) })
                }

            for try await vouchers in stream {
                voucherStatusSubscriptionTask?.cancel()

                guard !vouchers.isEmpty else {
                    logger.debug("Voucher sync stopped")
                    continue
                }
                try Task.checkCancellation()

                logger.debug("Voucher sync started: \(vouchers.count)")
                startLocationSync(for: vouchers)
            }
        }
    }

    override public func stopSyncUp() {
        localVouchersMonitoringTask?.cancel()
        voucherStatusSubscriptionTask?.cancel()
    }
}

extension VoucherLocationService {
    private func startLocationSync(for vouchers: [Voucher]) {
        voucherStatusSubscriptionTask = Task { [weak self] in
            do {
                try await self?.runLocationPipeline(for: vouchers)
            } catch {
                // Superseded by a newer tracked-voucher set: whatever the pipeline threw on the way out,
                // an operation cancelled underneath it included, is not a failure.
                guard !Task.isCancelled else { return }
                self?.logger.error("Voucher sync failed during monitoring: \(error)")
            }
        }
    }

    /// The reactive two-phase pipeline. `scan` keeps a complete positions snapshot up to date from the
    /// member subscription's deltas; `flatMapLatest` re-derives the ring-status subscription whenever the
    /// positions change and `scan`s its deltas into a complete status snapshot. Joining the two is a pure
    /// function, so nothing needs to be reconciled by hand between emissions.
    private func runLocationPipeline(for vouchers: [Voucher]) async throws {
        let memberReqs = try memberRequests(vouchers)
        guard !memberReqs.isEmpty else { return }

        // A runtime constant rather than storage, so it is read once per subscription, not per emission.
        let keysPerPage = try await runtimeService.fetchRingKeysPageSize()

        // Ring capacity is chain config keyed by denomination and cannot change within a session, so
        // it is resolved once per tracked set rather than per emission. A denomination that fails to
        // resolve simply has no fungibility written until it does.
        let capacities = try await ringCapacityProvider.capacities(
            for: Set(vouchers.map(\.exponent))
        )

        let memberStream: AnyAsyncSequence<MemberStatusResult> = CallbackBatchStorageSubscription
            .asyncStream(
                requests: memberReqs,
                connection: connection,
                runtimeService: runtimeService,
                logger: logger
            )

        // Member updates arrive keyed by the voucher's public key, the way they were subscribed.
        let indexByPublicKey = Dictionary(
            vouchers.map { ($0.publicKey, $0.derivationIndex) },
            uniquingKeysWith: { first, _ in first }
        )

        let positionsStream = memberStream
            .scan([CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>]()) { positions, result in
                var positions = positions
                for update in result.ringPositionUpdates {
                    guard let index = indexByPublicKey[update.publicKey] else { continue }
                    // The update was delivered, so it is `.defined`; a delivered empty reading is
                    // `.defined(nil)` (retraction) rather than a dropped key, so it can revert the voucher.
                    // A key never delivered stays absent — that, not `.undefined`, is "not changed".
                    positions[index] = .defined(update.ringPosition)
                }
                return positions
            }

        let resolvedStream = positionsStream
            .flatMapLatest { [weak self] positions -> AnyAsyncSequence<[CoinageKeyIndex: VoucherLocationUpdate]> in
                guard let self else {
                    return AsyncEmptySequence().eraseToAnyAsyncSequence()
                }
                return resolvedLocationsStream(
                    positions: positions,
                    vouchers: vouchers,
                    capacities: capacities,
                    keysPerPage: keysPerPage
                )
            }
            .removeDuplicates { $0 == $1 }

        for try await updates in resolvedStream {
            try Task.checkCancellation()
            try await write(updates)
        }
    }

    /// Subscribes to `RingKeysStatus` for the rings the current positions land in, `scan`s the deltas into
    /// a complete status snapshot, and resolves each voucher's location against it. When no voucher is yet
    /// placed in a ring, emits the onboarding-only resolution once so those writes still happen.
    private func resolvedLocationsStream(
        positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>],
        vouchers: [Voucher],
        capacities: [Int16: Int],
        keysPerPage: Int
    ) -> AnyAsyncSequence<[CoinageKeyIndex: VoucherLocationUpdate]> {
        let voucherByIndex = Dictionary(uniqueKeysWithValues: vouchers.map { ($0.derivationIndex, $0) })
        let recyclers = Self.recyclers(positions: positions, voucherByIndex: voucherByIndex)
        // One request per distinct ring, however many vouchers share it.
        let rings = Set(recyclers.values)
        let requests = ringStatusRequests(for: rings) + unloadedCountRequests(for: rings)

        guard !requests.isEmpty else {
            let resolved = Self.resolveLocations(positions: positions, statuses: [:], keysPerPage: keysPerPage)
            return AsyncJustSequence(Self.updates(
                locations: resolved,
                unloadedCounts: [:],
                voucherByIndex: voucherByIndex,
                capacities: capacities
            )).eraseToAnyAsyncSequence()
        }

        let statusStream: AnyAsyncSequence<MemberStatusResult> = CallbackBatchStorageSubscription
            .asyncStream(
                requests: requests,
                connection: connection,
                runtimeService: runtimeService,
                logger: logger
            )

        return statusStream
            .scan(RingSnapshot.empty) { snapshot, result in
                snapshot.applying(result)
            }
            .map { snapshot in
                // The readings are shared per ring; the resolution below is per voucher, so each
                // voucher reads the ring it sits in.
                let statuses = recyclers.compactMapValues { snapshot.statuses[$0] }
                let unloadedCounts = recyclers.compactMapValues { snapshot.unloadedCounts[$0] }

                return Self.updates(
                    locations: Self.resolveLocations(
                        positions: positions,
                        statuses: statuses,
                        keysPerPage: keysPerPage
                    ),
                    unloadedCounts: unloadedCounts,
                    voucherByIndex: voucherByIndex,
                    capacities: capacities
                )
            }
            .eraseToAnyAsyncSequence()
    }
}

// MARK: - Requests

private extension VoucherLocationService {
    /// Subscribes to `Members[identifier][voucherPubKey]` for every tracked voucher — including those
    /// already in a recycler, so a ring that keeps filling refreshes the voucher's position and member
    /// count. Detects Onboarding -> Included transitions and in-ring member growth.
    func memberRequests(_ vouchers: [Voucher]) throws -> [BatchStorageSubscriptionRequest] {
        vouchers.map { voucher in
            let publicKey = voucher.publicKey
            let collectionId = RecyclerCollectionIdentifier.identifier(instanceId: instanceId, for: voucher.exponent)
            let mappingKey = SubscriptionKey.member(publicKey: publicKey).mappingKey

            let innerRequest = DoubleMapSubscriptionRequest(
                storagePath: MembersPallet.Storage.members(),
                localKey: "",
                keyParamClosure: {
                    (
                        BytesCodable(wrappedValue: collectionId),
                        BytesCodable(wrappedValue: publicKey)
                    )
                }
            )

            return BatchStorageSubscriptionRequest(innerRequest: innerRequest, mappingKey: mappingKey)
        }
    }

    func ringStatusRequests(for rings: Set<RecyclerKey>) -> [BatchStorageSubscriptionRequest] {
        rings.map { ring in
            let collectionId = RecyclerCollectionIdentifier.identifier(
                instanceId: instanceId,
                for: ring.exponent
            )

            let innerRequest = DoubleMapSubscriptionRequest(
                storagePath: MembersPallet.Storage.ringKeysStatus(),
                localKey: "",
                keyParamClosure: {
                    (
                        BytesCodable(wrappedValue: collectionId),
                        StringCodable(wrappedValue: ring.index)
                    )
                }
            )

            return BatchStorageSubscriptionRequest(
                innerRequest: innerRequest,
                mappingKey: SubscriptionKey.ringStatus(recycler: ring).mappingKey
            )
        }
    }

    /// Subscribes to `RecyclersUnloadedCount` per ring. A plain map whose single key is a tuple, so
    /// it goes through `MapSubscriptionRequest` rather than the n-map path used for
    /// `RecyclerAliasStates`.
    func unloadedCountRequests(for rings: Set<RecyclerKey>) -> [BatchStorageSubscriptionRequest] {
        rings.map { ring in
            let key = RecyclerUnloadedCountKey(
                instanceId: instanceId,
                exponent: ring.exponent,
                ringIndex: ring.index
            )

            let innerRequest = MapSubscriptionRequest(
                storagePath: CoinagePallet.Storage.recyclersUnloadedCount(),
                localKey: "",
                keyParamClosure: { key }
            )

            return BatchStorageSubscriptionRequest(
                innerRequest: innerRequest,
                mappingKey: SubscriptionKey.unloadedCount(recycler: ring).mappingKey
            )
        }
    }
}

// MARK: - Ring readings

extension VoucherLocationService {
    /// The ring each placed voucher sits in. Vouchers that share a ring map to one ``RecyclerKey``,
    /// which is what collapses their subscriptions into a single request each.
    static func recyclers(
        positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>],
        voucherByIndex: [CoinageKeyIndex: Voucher]
    ) -> [CoinageKeyIndex: RecyclerKey] {
        positions.reduce(into: [:]) { recyclers, entry in
            guard case let .defined(.some(position)) = entry.value,
                  let ringIndex = position.ringIndex,
                  let voucher = voucherByIndex[entry.key]
            else { return }

            recyclers[entry.key] = RecyclerKey(exponent: voucher.exponent, index: ringIndex)
        }
    }

    /// The two per-ring readings, accumulated from the subscription's deltas. Kept together so one
    /// `scan` covers both and the join sees a consistent pair.
    struct RingSnapshot {
        var statuses: [RecyclerKey: UncertainStorage<MembersPallet.RingKeysStatus?>]
        var unloadedCounts: [RecyclerKey: UncertainStorage<UInt32?>]

        static let empty = RingSnapshot(statuses: [:], unloadedCounts: [:])

        func applying(_ result: MemberStatusResult) -> RingSnapshot {
            var snapshot = self

            for update in result.ringStatusUpdates {
                snapshot.statuses[update.recycler] = .defined(update.ringKeysStatus)
            }

            for update in result.unloadedCountUpdates {
                snapshot.unloadedCounts[update.recycler] = .defined(update.unloadedCount)
            }

            return snapshot
        }
    }
}

// MARK: - Resolution

extension VoucherLocationService {
    /// Pure join of the two snapshots into the location each voucher should be persisted with.
    ///
    /// The snapshots are read three ways: a key absent from the map (or ``UncertainStorage/undefined``) was
    /// not delivered in the subscription and is left untouched; a ``UncertainStorage/defined(_:)`` with a
    /// `nil` reading was delivered empty (absent on chain or retracted by a fork); and a `.defined` with a
    /// value carries it.
    ///
    /// - A member row delivered empty (`.defined(nil)`, retracted) reverts the voucher to `.unlocated` — the
    ///   fix for the case a fork drops a member and the stale in-recycler state would otherwise linger.
    /// - An included voucher whose ring status confirms its key becomes in-recycler with the real member
    ///   count; if that ring status is delivered empty (the ring was retracted) the voucher falls back to
    ///   onboarding.
    /// - A status not delivered yet leaves the voucher deferred (no entry emitted) rather than guessed at.
    /// - Anything else (onboarding/suspended position) is onboarding.
    static func resolveLocations(
        positions: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingPosition?>],
        statuses: [CoinageKeyIndex: UncertainStorage<MembersPallet.RingKeysStatus?>],
        keysPerPage: Int,
        observedAt: Date = .now
    ) -> [CoinageKeyIndex: Voucher.OnChainState] {
        positions.reduce(into: [:]) { resolved, entry in
            let (derivationIndex, positionEntry) = entry

            // Not delivered — leave the voucher as it is.
            guard case let .defined(deliveredPosition) = positionEntry else { return }

            // Delivered empty — the member row was retracted; revert to unlocated.
            guard let position = deliveredPosition else {
                resolved[derivationIndex] = .unlocated
                return
            }

            guard let ringIndex = position.ringIndex else {
                resolved[derivationIndex] = .onboarding
                return
            }

            switch statuses[derivationIndex] {
            case .none,
                 .undefined?:
                // Ring status not delivered yet — defer rather than guess.
                return
            case .defined(.none)?:
                // The ring status was delivered empty (ring retracted): fall back to onboarding.
                resolved[derivationIndex] = .onboarding
            case let .defined(.some(status))?:
                guard status.includesKey(from: position, keysPerPage: keysPerPage) else { return }
                resolved[derivationIndex] = .inRecycler(
                    Voucher.Recycler(index: ringIndex, membersCount: status.included, enteredAt: observedAt)
                )
            }
        }
    }
}

extension VoucherLocationService {
    /// Pairs each resolved location with its ring's fungibility.
    ///
    /// A score is only attached when the ring's own readings are all in hand: the voucher resolved
    /// into a ring, its unloaded count was actually delivered, and its denomination's capacity is
    /// known. Anything short of that leaves the stored score alone rather than guessing an
    /// optimistic one — this drives a privacy indicator, so an absent reading must not read as
    /// "nothing unloaded".
    static func updates(
        locations: [CoinageKeyIndex: Voucher.OnChainState],
        unloadedCounts: [CoinageKeyIndex: UncertainStorage<UInt32?>],
        voucherByIndex: [CoinageKeyIndex: Voucher],
        capacities: [Int16: Int]
    ) -> [CoinageKeyIndex: VoucherLocationUpdate] {
        locations.reduce(into: [:]) { updates, entry in
            let (derivationIndex, location) = entry

            updates[derivationIndex] = VoucherLocationUpdate(
                derivationIndex: derivationIndex,
                remoteState: location,
                recyclerFungibility: nil,
                maxRecyclerFungibility: nil
            )

            guard case let .inRecycler(recycler) = location,
                  // Delivered-and-absent is a real zero: the pallet creates the entry on the first
                  // unload, so "no entry" means nothing has been unloaded from this ring yet.
                  case let .defined(delivered) = unloadedCounts[derivationIndex] ?? .undefined,
                  let exponent = voucherByIndex[derivationIndex]?.exponent,
                  let capacity = capacities[exponent]
            else { return }

            let unloaded = delivered ?? 0

            updates[derivationIndex] = VoucherLocationUpdate(
                derivationIndex: derivationIndex,
                remoteState: location,
                recyclerFungibility: RecyclerFungibility.current(
                    included: recycler.membersCount,
                    unloaded: unloaded,
                    capacity: capacity
                ),
                maxRecyclerFungibility: RecyclerFungibility.maximum(
                    included: recycler.membersCount,
                    unloaded: unloaded,
                    capacity: capacity
                )
            )
        }
    }
}

// MARK: - Persistence

private extension VoucherLocationService {
    func write(_ updates: [CoinageKeyIndex: VoucherLocationUpdate]) async throws {
        guard !updates.isEmpty else { return }

        // A dedicated write-only mapper touches only the location and fungibility columns, so a
        // concurrent change to any other voucher field is not clobbered by this write.
        let values = Array(updates.values)

        try await databaseFactory.makeVoucherLocationRepository()
            .saveOperation({ values }, { [] })
            .asyncExecute()
        logger.debug("Updated \(values.count) voucher locations via subscription")
    }
}
