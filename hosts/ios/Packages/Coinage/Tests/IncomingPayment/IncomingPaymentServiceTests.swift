import Testing
import Foundation
import AsyncExtensions
import SubstrateSdk
import os
@testable import Coinage

struct IncomingPaymentServiceTests {
    /// Inert denomination context — the claim stubs ignore it; it only exists to drive `setup(with:)`.
    private static let denomination = DenominationBreakdownContext(
        unit: 1,
        precision: 10,
        maxExponent: 0,
        minExponent: 0
    )

    private func makeService(
        store: InMemoryIncomingPaymentStore,
        secretStore: InMemoryIncomingPaymentSecretStore = InMemoryIncomingPaymentSecretStore(),
        resolver: StubSourceResolver = StubSourceResolver(),
        acknowledger: StubAcknowledger = StubAcknowledger(),
        claim: any ClaimCoinsServicing = StubClaimCoinsService(),
        verdictResolver: any CoinageGroupVerdictResolving = StubGroupVerdictResolver(verdict: .notClaimed),
        lifecycle: CoinageLifecycle? = nil,
        ownerId: Data = Data([0xA0])
    ) -> IncomingPaymentService {
        IncomingPaymentService(store: store,
        secretStore: secretStore,
        sourceResolver: resolver,
        paymentContext: IncomingPaymentContext(logger: StubLogger()),
        claimCoinsService: claim,
        claimAssetService: StubClaimAssetService(),
        verdictResolver: verdictResolver,
        acknowledger: acknowledger,
        instanceId: 0,
        logger: StubLogger(),
        lifecycle: lifecycle, ownerId: ownerId)
    }

    @Test(.timeLimit(.minutes(1)))
    func zeroCoinsClaimsFullSourceAndPersistsRealZero() async throws {
        let store = InMemoryIncomingPaymentStore()
        let secretStore = InMemoryIncomingPaymentSecretStore()
        let service = makeService(
            store: store,
            secretStore: secretStore,
            claim: StubClaimCoinsService(detections: [.claiming, .claimed(amount: 40, finalized: true)])
        )

        try await service.accept(
            amount: 0,
            descriptor: .coins(secretKeys: [Data([0x01])]),
            paymentId: "p1",
            productId: "prod"
        )
        #expect(store.payment(for: "top up:prod:p1")?.amount == 0)

        service.setup(with: Self.denomination)
        defer { service.throttle() }
        try await waitUntil { store.payment(for: "top up:prod:p1")?.outcome != nil }

        #expect(store.payment(for: "top up:prod:p1")?.outcome == .claimed)
        #expect(store.payment(for: "top up:prod:p1")?.amount == 0)
        let restarted = makeService(store: store, secretStore: secretStore)
        let stream = try await restarted.subscribeStatus(for: "p1", productId: "prod")
        for try await status in stream {
            #expect(status == .claimed(finalized: true))
            break
        }
    }

    @Test(arguments: [
        IncomingPaymentSourceDescriptor.privateKey(secretKey: Data([0x01])),
        .productAccount(derivationPath: "//product//prod/0x1")
    ])
    func acceptRejectsZeroForWalletSources(descriptor: IncomingPaymentSourceDescriptor) async throws {
        let store = InMemoryIncomingPaymentStore()
        let secretStore = InMemoryIncomingPaymentSecretStore()
        let service = makeService(store: store, secretStore: secretStore)

        await #expect {
            try await service.accept(amount: 0, descriptor: descriptor, paymentId: "p1", productId: "prod")
        } throws: { ($0 as? IncomingPaymentError) == .invalidAmount }
        #expect(store.payment(for: "top up:prod:p1") == nil)
        #expect(!secretStore.hasDescriptor(for: "top up:prod:p1"))
    }

    @Test(.timeLimit(.minutes(1)))
    func suspendedOldAcceptCannotOverwriteReactivatedPaymentOrRemoveItsSecret() async throws {
        let (entered, didEnter) = AsyncStream<Void>.makeStream()
        let (resume, release) = AsyncStream<Void>.makeStream()
        let firstSave = OSAllocatedUnfairLock(initialState: true)
        let store = InMemoryIncomingPaymentStore(beforeSave: {
            let suspend = firstSave.withLock {
                let first = $0
                $0 = false
                return first
            }
            guard suspend else { return }
            didEnter.yield(())
            for await _ in resume { break }
        })
        let secrets = InMemoryIncomingPaymentSecretStore()
        let lifecycle = try CoinageLifecycle(
            rootEntropyManager: MockEntropyManager(entropy: Data(repeating: 2, count: 32))
        )
        #expect(lifecycle.setActive(true))
        let oldService = makeService(store: store, secretStore: secrets, lifecycle: lifecycle)
        let oldAccept = Task {
            do {
                try await oldService.accept(
                    amount: 100, descriptor: .coins(secretKeys: [Data([1])]), paymentId: "p", productId: "prod"
                )
                return false
            } catch {
                return true
            }
        }
        defer {
            release.finish()
            oldAccept.cancel()
        }
        var entries = entered.makeAsyncIterator()
        _ = await entries.next()
        #expect(lifecycle.setActive(false))
        #expect(lifecycle.setActive(true))

        let replacement = makeService(store: store, secretStore: secrets, lifecycle: lifecycle)
        let descriptor = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([2])])
        try await replacement.accept(amount: 200, descriptor: descriptor, paymentId: "p", productId: "prod")
        release.yield(())
        #expect(await oldAccept.value)

        #expect(store.payment(for: "top up:prod:p")?.amount == 200)
        #expect(try secrets.fetch(groupId: "top up:prod:p") == descriptor)
        #expect(secrets.removedGroupIds().isEmpty)
    }

    @Test(.timeLimit(.minutes(1)), arguments: [Data?.none, Data([0xB0])])
    func foreignAndLegacyRecordsArePreservedButNeverAdopted(recordOwner: Data?) async throws {
        let unowned = IncomingPayment(
            paymentId: "foreign", productId: "prod", amount: 0, createdAt: Date(), outcome: nil,
            ownerId: recordOwner
        )
        let owned = IncomingPayment(
            paymentId: "owned", productId: "prod", amount: 0, createdAt: Date(), outcome: nil,
            ownerId: Data([0xA0])
        )
        let descriptor = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([1])])
        let store = InMemoryIncomingPaymentStore(seed: [unowned, owned])
        let secrets = InMemoryIncomingPaymentSecretStore(seed: [
            unowned.groupId: descriptor,
            owned.groupId: .coins(secretKeys: [Data([2])])
        ])
        let service = makeService(
            store: store, secretStore: secrets, claim: StubClaimCoinsService(detections: [.notClaimed])
        )
        await #expect {
            _ = try await service.subscribeStatus(for: "foreign", productId: "prod")
        } throws: { ($0 as? IncomingPaymentError) == .notFound("foreign") }
        await #expect {
            try await service.accept(
                amount: 0, descriptor: .coins(secretKeys: [Data([3])]), paymentId: "foreign", productId: "prod"
            )
        } throws: { ($0 as? IncomingPaymentError) == .alreadyExists }

        service.setup(with: Self.denomination)
        defer { service.throttle() }
        try await waitUntil { store.payment(for: owned.groupId)?.outcome != nil }
        #expect(store.payment(for: unowned.groupId) == unowned)
        #expect(try secrets.fetch(groupId: unowned.groupId) == descriptor)
        #expect(!secrets.removedGroupIds().contains(unowned.groupId))
        #expect(!store.settledGroupIds().contains(unowned.groupId))
    }

    @Test func acceptFailsWhenBusyCheckCannotReadSecrets() async throws {
        let active = IncomingPayment(paymentId: "p1",
        productId: "prod",
        amount: 100,
        createdAt: Date(),
        outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [active])
        let secretStore = InMemoryIncomingPaymentSecretStore()
        secretStore.fetchError = InMemoryIncomingPaymentSecretStore.Failure()
        let service = makeService(store: store, secretStore: secretStore)

        await #expect {
            try await service.accept(
                amount: 50,
                descriptor: .coins(secretKeys: [Data([0x05])]),
                paymentId: "p2",
                productId: "prod"
            )
        } throws: { Self.isUnknown($0) }
        #expect(store.payment(for: "top up:prod:p2") == nil)
    }

    @Test func acceptPersistsRecordAndSecret() async throws {
        let store = InMemoryIncomingPaymentStore()
        let secretStore = InMemoryIncomingPaymentSecretStore()
        let service = makeService(store: store, secretStore: secretStore)

        try await service.accept(
            amount: 100,
            descriptor: .coins(secretKeys: [Data([0x01])]),
            paymentId: "p1",
            productId: "prod"
        )

        let saved = try #require(store.payment(for: "top up:prod:p1"))
        #expect(saved.amount == 100)
        #expect(saved.outcome == nil)
        #expect(saved.groupId == "top up:prod:p1")
        #expect(secretStore.hasDescriptor(for: "top up:prod:p1"))
    }

    @Test func acceptRejectsDuplicateGroupId() async throws {
        let existing = IncomingPayment(paymentId: "p1",
        productId: "prod",
        amount: 100,
        createdAt: Date(),
        outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [existing])
        let service = makeService(store: store)

        await #expect {
            try await service.accept(
                amount: 100,
                descriptor: .coins(secretKeys: [Data([0x01])]),
                paymentId: "p1",
                productId: "prod"
            )
        } throws: { ($0 as? IncomingPaymentError) == .alreadyExists }
    }

    @Test func acceptRejectsBusySource() async throws {
        let active = IncomingPayment(paymentId: "p1",
        productId: "prod",
        amount: 100,
        createdAt: Date(),
        outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [active])
        let secretStore = InMemoryIncomingPaymentSecretStore(
            seed: ["top up:prod:p1": .coins(secretKeys: [Data([0x05])])]
        )
        let service = makeService(store: store, secretStore: secretStore)

        await #expect {
            try await service.accept(
                amount: 50,
                descriptor: .coins(secretKeys: [Data([0x05])]),
                paymentId: "p2",
                productId: "prod"
            )
        } throws: { ($0 as? IncomingPaymentError) == .sourceBusy }
    }

    @Test func sameProductAccountPathIsBusy() async throws {
        let active = IncomingPayment(paymentId: "p1", productId: "prod", amount: 100, createdAt: Date(), outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [active])
        let secretStore = InMemoryIncomingPaymentSecretStore(
            seed: ["top up:prod:p1": .productAccount(derivationPath: "//product//prod/0x1")]
        )
        let service = makeService(store: store, secretStore: secretStore)

        await #expect {
            try await service.accept(
                amount: 50,
                descriptor: .productAccount(derivationPath: "//product//prod/0x1"),
                paymentId: "p2",
                productId: "prod"
            )
        } throws: { ($0 as? IncomingPaymentError) == .sourceBusy }
    }

    @Test func differentProductAccountPathsAreNotBusy() async throws {
        // The path embeds the product, so another product's account at the same index is other money.
        let active = IncomingPayment(paymentId: "p1", productId: "prodA", amount: 100, createdAt: Date(), outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [active])
        let secretStore = InMemoryIncomingPaymentSecretStore(
            seed: ["top up:prodA:p1": .productAccount(derivationPath: "//product//prodA/0x1")]
        )
        let service = makeService(store: store, secretStore: secretStore)

        try await service.accept(
            amount: 50,
            descriptor: .productAccount(derivationPath: "//product//prodB/0x1"),
            paymentId: "p1",
            productId: "prodB"
        )
        #expect(store.payment(for: "top up:prodB:p1") != nil)
    }

    @Test func busyCheckSkipsCorruptedSecrets() async throws {
        // A payment whose secret no longer decodes can never claim, so it holds nothing busy and must
        // not block every future accept.
        let active = IncomingPayment(paymentId: "p1", productId: "prod", amount: 100, createdAt: Date(), outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [active])
        let secretStore = InMemoryIncomingPaymentSecretStore()
        secretStore.corruptedGroupIds = ["top up:prod:p1"]
        let service = makeService(store: store, secretStore: secretStore)

        try await service.accept(
            amount: 50,
            descriptor: .coins(secretKeys: [Data([0x05])]),
            paymentId: "p2",
            productId: "prod"
        )
        #expect(store.payment(for: "top up:prod:p2") != nil)
    }

    @Test func settledSourceIsNotBusy() async throws {
        let done = IncomingPayment(paymentId: "p1",
        productId: "prod",
        amount: 100,
        createdAt: Date(),
        outcome: .claimed, ownerId: Data([0xA0]))
        // A settled payment has no live secret — it was wiped on settle.
        let store = InMemoryIncomingPaymentStore(seed: [done])
        let service = makeService(store: store)

        // Reusing a source whose only prior payment is terminal is allowed.
        try await service.accept(
            amount: 50,
            descriptor: .coins(secretKeys: [Data([0x06])]),
            paymentId: "p2",
            productId: "prod"
        )
        #expect(store.payment(for: "top up:prod:p2") != nil)
    }

    @Test func acceptRejectsInvalidSource() async throws {
        let store = InMemoryIncomingPaymentStore()
        let service = makeService(store: store, resolver: StubSourceResolver(shouldFail: true))

        await #expect {
            try await service.accept(
                amount: 10,
                descriptor: .privateKey(secretKey: Data([0x00])),
                paymentId: "p1",
                productId: "prod"
            )
        } throws: { Self.isInvalidSource($0) }
    }

    @Test func rejectedAcceptLeavesNoSecretBehind() async throws {
        let store = InMemoryIncomingPaymentStore()
        let secretStore = InMemoryIncomingPaymentSecretStore()
        let service = makeService(
            store: store,
            secretStore: secretStore,
            resolver: StubSourceResolver(shouldFail: true)
        )

        _ = try? await service.accept(
            amount: 10,
            descriptor: .privateKey(secretKey: Data([0x00])),
            paymentId: "p1",
            productId: "prod"
        )
        #expect(!secretStore.hasDescriptor(for: "top up:prod:p1"))
    }

    @Test func acceptMapsUnexpectedFailureToUnknown() async throws {
        let store = InMemoryIncomingPaymentStore()
        store.fetchError = InMemoryIncomingPaymentStore.Failure()
        let service = makeService(store: store)

        await #expect {
            try await service.accept(
                amount: 10,
                descriptor: .coins(secretKeys: [Data([0x07])]),
                paymentId: "p1",
                productId: "prod"
            )
        } throws: { Self.isUnknown($0) }
    }

    @Test func coldSubscribeReturnsStoredVerdictExactly() async throws {
        let settled = IncomingPayment(paymentId: "p1",
        productId: "prod",
        amount: 100,
        createdAt: Date(),
        outcome: .claimedPartially(actualClaimed: 42), ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [settled])
        let service = makeService(store: store)

        let stream = try await service.subscribeStatus(for: "p1", productId: "prod")
        var observed: IncomingPaymentStatus?
        for try await status in stream {
            observed = status
            break
        }
        #expect(observed == .claimedPartially(actualClaimed: 42))
    }

    @Test func coldSubscribeToUnknownPaymentThrowsNotFound() async throws {
        let store = InMemoryIncomingPaymentStore()
        let service = makeService(store: store)

        await #expect {
            _ = try await service.subscribeStatus(for: "ghost", productId: "prod")
        } throws: { ($0 as? IncomingPaymentError) == .notFound("ghost") }
    }

    @Test(.timeLimit(.minutes(1)))
    func partialOutcomeIsAcknowledgedAndPersisted() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(
            detections: [.claiming, .claimedPartially(claimed: 40)],
            acknowledger: acknowledger
        )

        rig.service.setup(with: Self.denomination)
        try await waitUntil { acknowledger.calls().count == 1 }
        rig.service.throttle()

        let call = try #require(acknowledger.calls().first)
        #expect(call.outcome == .claimedPartially(actualClaimed: 40))
        #expect(call.requestedAmount == 100)
        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .claimedPartially(actualClaimed: 40))
    }

    @Test(.timeLimit(.minutes(1)))
    func finalizedFullSourceBelowMinimumPersistsActualShortfall() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(
            detections: [.claimed(amount: 40, finalized: true)],
            acknowledger: acknowledger
        )

        rig.service.setup(with: Self.denomination)
        defer { rig.service.throttle() }
        try await waitUntil { acknowledger.calls().count == 1 }

        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .claimedPartially(actualClaimed: 40))
        #expect(acknowledger.calls().first?.requestedAmount == 100)
        #expect(acknowledger.calls().first?.outcome == .claimedPartially(actualClaimed: 40))
        let stream = try await rig.service.subscribeStatus(for: "p", productId: "prod")
        for try await status in stream {
            #expect(status == .claimedPartially(actualClaimed: 40))
            break
        }
    }

    @Test(.timeLimit(.minutes(1)), arguments: [Balance(0), Balance(100)])
    func terminalPartialSourceMeetingMinimumIsClaimed(amount: Balance) async throws {
        let rig = makeDrivingService(detections: [.claimedPartially(claimed: 120)], amount: amount)

        rig.service.setup(with: Self.denomination)
        defer { rig.service.throttle() }
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome != nil }

        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .claimed)
    }

    @Test(.timeLimit(.minutes(1)))
    func zeroCoinsWithoutFundsIsNotClaimed() async throws {
        let rig = makeDrivingService(detections: [.detecting, .notClaimed], amount: 0)

        rig.service.setup(with: Self.denomination)
        defer { rig.service.throttle() }
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome != nil }

        #expect(rig.store.payment(for: "top up:prod:p")?.amount == 0)
        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .notClaimed)
    }

    @Test(.timeLimit(.minutes(1)), arguments: [Balance(0), Balance(100)])
    func unfinalizedCreditAndReorgRemainActiveUntilStableVerdict(amount: Balance) async throws {
        let store = InMemoryIncomingPaymentStore()
        let secrets = InMemoryIncomingPaymentSecretStore()
        let claim = StreamingClaimCoinsService()
        let service = makeService(store: store, secretStore: secrets, claim: claim)
        try await service.accept(
            amount: amount, descriptor: .coins(secretKeys: [Data([1])]), paymentId: "p", productId: "prod"
        )
        let stream = try await service.subscribeStatus(for: "p", productId: "prod")
        var statuses = stream.makeAsyncIterator()
        #expect(try await statuses.next() == .detecting)
        service.setup(with: Self.denomination)
        defer { service.throttle() }

        claim.detections.yield(.claimingRest(claimed: 120))
        #expect(try await statuses.next() == .claiming)
        claim.detections.yield(.claimed(amount: 40, finalized: false))
        let pending: IncomingPaymentStatus = amount == 0 ? .claimed(finalized: false) : .claiming
        #expect(try await statuses.next() == pending)
        #expect(store.payment(for: "top up:prod:p")?.outcome == nil)
        #expect(secrets.hasDescriptor(for: "top up:prod:p"))

        claim.detections.yield(.detecting)
        #expect(try await statuses.next() == .detecting)
        #expect(store.payment(for: "top up:prod:p")?.outcome == nil)
        claim.detections.yield(.claimed(amount: 40, finalized: true))
        claim.detections.finish()
        let outcome: IncomingPaymentTerminalOutcome = amount == 0 ? .claimed : .claimedPartially(actualClaimed: 40)
        #expect(try await statuses.next() == IncomingPaymentStatus(outcome: outcome))
        try await waitUntil { store.payment(for: "top up:prod:p")?.outcome != nil }
        #expect(store.payment(for: "top up:prod:p")?.outcome == outcome)
    }

    @Test(.timeLimit(.minutes(1)), arguments: [Balance(0), Balance(100)])
    func restartWithoutSecretRequiresActualFinalizedLedgerCredit(amount: Balance) async throws {
        let payment = IncomingPayment(paymentId: "p", productId: "prod", amount: amount, createdAt: Date(), outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [payment])
        let repository = MockCoinageTxRepository()
        let coin = Coin(exponent: 3, derivationIndex: 1, age: nil, publicKey: testKey(1))
        try await repository.register(.fixture(
            outputs: [.coin(1, coin.publicKey)], status: .finalizedSuccess, groupId: payment.groupId
        ))
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(store: repository),
            coinService: InMemoryCoinService(coins: [coin]),
            voucherService: InMemoryVoucherService()
        )
        let service = makeService(store: store, verdictResolver: resolver)
        let denomination = DenominationBreakdownContext(unit: 1, precision: 0, maxExponent: 3, minExponent: 0)

        service.setup(with: denomination)
        defer { service.throttle() }
        try await waitUntil { store.payment(for: payment.groupId)?.outcome != nil }

        let outcome: IncomingPaymentTerminalOutcome = amount == 0 ? .claimed : .claimedPartially(actualClaimed: 8)
        #expect(store.payment(for: payment.groupId)?.outcome == outcome)
        #expect(store.payment(for: payment.groupId)?.amount == amount)
        let restarted = makeService(store: store)
        let stream = try await restarted.subscribeStatus(for: "p", productId: "prod")
        for try await status in stream {
            #expect(status == IncomingPaymentStatus(outcome: outcome))
            break
        }
    }

    @Test(.timeLimit(.minutes(1)))
    func restartZeroWithoutSecretOrLedgerCreditIsNotClaimed() async throws {
        let payment = IncomingPayment(paymentId: "p", productId: "prod", amount: 0, createdAt: Date(), outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [payment])
        let resolver = CoinageGroupVerdictResolver(
            txService: MockCoinageTxService(),
            coinService: InMemoryCoinService(),
            voucherService: InMemoryVoucherService()
        )
        let service = makeService(store: store, verdictResolver: resolver)

        service.setup(with: Self.denomination)
        defer { service.throttle() }
        try await waitUntil { store.payment(for: payment.groupId)?.outcome != nil }

        #expect(store.payment(for: payment.groupId)?.outcome == .notClaimed)
    }

    @Test(.timeLimit(.minutes(1)))
    func notClaimedKeepsAReadableSourceSecret() async throws {
        let rig = makeDrivingService(detections: [.notClaimed])

        #expect(rig.secretStore.hasDescriptor(for: "top up:prod:p"))

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome == .notClaimed }
        rig.service.throttle()

        #expect(rig.secretStore.hasDescriptor(for: "top up:prod:p"))
        #expect(rig.secretStore.removedGroupIds().isEmpty)
    }

    /// The other half of the same rule: a verdict that says where the funds went does retire the key.
    @Test(.timeLimit(.minutes(1)))
    func settleWipesSourceSecretOnceClaimed() async throws {
        let rig = makeDrivingService(detections: [.claimed(amount: 100, finalized: true)])

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome == .claimed }
        rig.service.throttle()

        #expect(!rig.secretStore.hasDescriptor(for: "top up:prod:p"))
        #expect(rig.secretStore.removedGroupIds() == ["top up:prod:p"])
    }

    @Test(.timeLimit(.minutes(1)))
    func unreadableSecretLeavesPaymentActive() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(detections: [.notClaimed], acknowledger: acknowledger)
        rig.secretStore.fetchError = InMemoryIncomingPaymentSecretStore.Failure()

        rig.service.setup(with: Self.denomination)
        try await Task.sleep(for: .milliseconds(200))
        rig.service.throttle()

        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == nil)
        #expect(rig.store.settledGroupIds().isEmpty)
        #expect(acknowledger.calls().isEmpty)
        #expect(rig.secretStore.removedGroupIds().isEmpty)
        #expect(rig.verdictResolver.askedGroupIds().isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func lostSecretSettlesFromDurabilityGroup() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(
            detections: [.claimed(amount: 100, finalized: true)],
            acknowledger: acknowledger,
            secretPresent: false,
            durabilityVerdict: .success(.claimedPartially(claimed: 40))
        )

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome != nil }
        rig.service.throttle()

        #expect(rig.verdictResolver.askedGroupIds() == ["top up:prod:p"])
        #expect(rig.claim.retryUntil() == nil)
        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .claimedPartially(actualClaimed: 40))
        #expect(acknowledger.calls().map(\.outcome) == [.claimedPartially(actualClaimed: 40)])
    }

    @Test(.timeLimit(.minutes(1)))
    func lostSecretWithUnobservableGroupLeavesPaymentActive() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(
            detections: [.notClaimed],
            acknowledger: acknowledger,
            secretPresent: false,
            durabilityVerdict: .failure(StubGroupVerdictResolver.Unobservable())
        )

        rig.service.setup(with: Self.denomination)
        try await waitUntil { !rig.verdictResolver.askedGroupIds().isEmpty }
        try await Task.sleep(for: .milliseconds(100))
        rig.service.throttle()

        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == nil)
        #expect(acknowledger.calls().isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func unpersistedVerdictKeepsSecretAndDoesNotAcknowledge() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(detections: [.notClaimed], acknowledger: acknowledger)
        rig.store.settleError = InMemoryIncomingPaymentStore.Failure()

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.claim.retryUntil() != nil }
        try await Task.sleep(for: .milliseconds(200))
        rig.service.throttle()

        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == nil)
        #expect(rig.secretStore.hasDescriptor(for: "top up:prod:p"))
        #expect(rig.secretStore.removedGroupIds().isEmpty)
        #expect(acknowledger.calls().isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func corruptedSecretSettlesFromDurabilityGroup() async throws {
        let acknowledger = StubAcknowledger()
        let rig = makeDrivingService(
            detections: [.claimed(amount: 100, finalized: true)],
            acknowledger: acknowledger,
            durabilityVerdict: .success(.notClaimed)
        )
        rig.secretStore.corruptedGroupIds = ["top up:prod:p"]

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome != nil }
        rig.service.throttle()

        // Never re-run from a secret that will not read on any launch; settled from the ledger, and
        // the unreadable entry is wiped with the verdict.
        #expect(rig.claim.retryUntil() == nil)
        #expect(rig.verdictResolver.askedGroupIds() == ["top up:prod:p"])
        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .notClaimed)
        #expect(rig.secretStore.removedGroupIds() == ["top up:prod:p"])
        #expect(acknowledger.calls().map(\.outcome) == [.notClaimed])
    }

    @Test(.timeLimit(.minutes(1)))
    func walletSourceIsClaimedThroughTheAssetService() async throws {
        let createdAt = Date(timeIntervalSince1970: 1_000_000)
        let assetClaim = StubClaimAssetService(detections: [.claiming, .claimed(amount: 100, finalized: true)])
        let rig = makeDrivingService(
            detections: [.notClaimed],
            createdAt: createdAt,
            descriptor: .privateKey(secretKey: Data(repeating: 0x05, count: 32)),
            instanceId: 7,
            assetClaim: assetClaim
        )

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.store.payment(for: "top up:prod:p")?.outcome != nil }
        rig.service.throttle()

        let call = try #require(assetClaim.calls().first)
        #expect(call.amount == 100)
        #expect(call.groupId == "top up:prod:p")
        #expect(call.instanceId == 7)
        #expect(call.retryUntil == createdAt.addingTimeInterval(CoinageConstants.topUpRetryWindow))
        #expect(rig.claim.retryUntil() == nil)
        #expect(rig.store.payment(for: "top up:prod:p")?.outcome == .claimed)
    }

    @Test(.timeLimit(.minutes(1)))
    func claimRetryWindowIsTheOperationsOwn() async throws {
        let createdAt = Date(timeIntervalSince1970: 1_000_000)
        let rig = makeDrivingService(detections: [.notClaimed], createdAt: createdAt)

        rig.service.setup(with: Self.denomination)
        try await waitUntil { rig.claim.retryUntil() != nil }
        rig.service.throttle()

        let retryUntil = try #require(rig.claim.retryUntil())
        #expect(retryUntil == createdAt.addingTimeInterval(CoinageConstants.topUpRetryWindow))
    }

    struct DrivingRig {
        let service: IncomingPaymentService
        let store: InMemoryIncomingPaymentStore
        let secretStore: InMemoryIncomingPaymentSecretStore
        let claim: StubClaimCoinsService
        let verdictResolver: StubGroupVerdictResolver
        let payment: IncomingPayment
    }

    /// A service wired to drive one seeded `(prod, p)` payment through a canned detection sequence to
    /// settlement: `detections` feed the coins claim, `assetClaim` answers a wallet `descriptor`.
    /// Without a secret the durability fallback answers `durabilityVerdict`.
    private func makeDrivingService(
        detections: [CoinageTransferDetection],
        amount: Balance = 100,
        acknowledger: StubAcknowledger = StubAcknowledger(),
        createdAt: Date = Date(),
        secretPresent: Bool = true,
        durabilityVerdict: Result<CoinageTransferDetection, Error> = .success(.notClaimed),
        descriptor: IncomingPaymentSourceDescriptor = .coins(secretKeys: [Data([0x01])]),
        instanceId: CoinageInstanceId = 0,
        assetClaim: StubClaimAssetService = StubClaimAssetService()
    ) -> DrivingRig {
        let payment = IncomingPayment(paymentId: "p",
        productId: "prod",
        amount: amount,
        createdAt: createdAt,
        outcome: nil, ownerId: Data([0xA0]))
        let store = InMemoryIncomingPaymentStore(seed: [payment])
        let secretStore = InMemoryIncomingPaymentSecretStore(
            seed: secretPresent ? ["top up:prod:p": descriptor] : [:]
        )
        let claim = StubClaimCoinsService(detections: detections)
        let verdictResolver =
            switch durabilityVerdict {
            case let .success(verdict): StubGroupVerdictResolver(verdict: verdict)
            case let .failure(error): StubGroupVerdictResolver(error: error)
            }
        let service = IncomingPaymentService(store: store,
        secretStore: secretStore,
        sourceResolver: StubSourceResolver(),
        paymentContext: IncomingPaymentContext(logger: StubLogger()),
        claimCoinsService: claim,
        claimAssetService: assetClaim,
        verdictResolver: verdictResolver,
        acknowledger: acknowledger,
        instanceId: instanceId,
        logger: StubLogger(), ownerId: Data([0xA0]))
        return DrivingRig(
            service: service,
            store: store,
            secretStore: secretStore,
            claim: claim,
            verdictResolver: verdictResolver,
            payment: payment
        )
    }

    private func waitUntil(
        _ condition: @escaping () -> Bool,
        timeout: Duration = .seconds(100)
    ) async throws {
        let deadline = ContinuousClock.now.advanced(by: timeout)
        while ContinuousClock.now < deadline {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
    }

    private static func isInvalidSource(_ error: any Error) -> Bool {
        guard let error = error as? IncomingPaymentError, case .invalidSource = error else { return false }
        return true
    }

    private static func isUnknown(_ error: any Error) -> Bool {
        guard let error = error as? IncomingPaymentError, case .unknown = error else { return false }
        return true
    }
}
