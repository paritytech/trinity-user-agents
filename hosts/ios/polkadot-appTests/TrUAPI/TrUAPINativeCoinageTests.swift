import BigInt
import Coinage
import Foundation
import NovaCrypto
import Testing
import TrUAPIHost
@testable import polkadot_app

struct TrUAPINativeCoinageTests {
    private let scope = NativeCoinageScope(rootPublicKey: Data(repeating: 1, count: 32),
                                           genesisHash: Data(repeating: 2, count: 32), coinageInstanceId: 7)

    @Test func restartBeforeTransportCommitReplaysRetainedMemoWithoutAnotherDebit() async throws {
        let harness = try NativeWalletHarness()
        let journal = NativeRecordMemory()
        let presenter = NativeReviewHarness()
        let first = adapter(harness, journal, presenter)
        let intent = intent()
        let response = try await first.nativeCoinage(request: request(.preparePayment(intent: intent)))
        guard case let .prepared(payment, memo) = response else { Issue.record("Expected prepared custody"); return }
        #expect(payment.state == .preparing)
        #expect(memo != nil)
        // Crash after native custody, before Host ciphertext acceptance/CommitHandoff. New adapter, same stores.
        let restarted = adapter(harness, journal, presenter)
        let replay = try await restarted.nativeCoinage(request: request(.preparePayment(intent: intent)))
        guard case let .prepared(replayed, replayMemo) = replay else { Issue.record("Expected replay"); return }
        #expect(replayed == payment)
        #expect(replayMemo == memo)
        #expect(await harness.debits == 1)
        #expect(await presenter.reviews.count == 1)
        let beforeAccept = try await restarted.nativeCoinage(request: request(.pendingHandoffs(
            productId: intent.productId, acceptedOperations: []
        )))
        #expect(beforeAccept == .payments(payments: []))
        // Host's durable acceptance ledger repairs an ambiguous CommitHandoff without spending again.
        let repaired = try await restarted.nativeCoinage(request: request(.pendingHandoffs(
            productId: intent.productId, acceptedOperations: [intent.operationId]
        )))
        guard case let .payments(cards) = repaired else { Issue.record("Expected pending handoff"); return }
        #expect(cards.map(\.state) == [.delivering])
        _ = try await restarted.nativeCoinage(request: request(.noteDelivery(productId: intent.productId, operationId: intent.operationId)))
        let delivered = try await restarted.nativeCoinage(request: request(.views(productId: intent.productId)))
        guard case let .payments(deliveredCards) = delivered else { Issue.record("Expected public cards"); return }
        #expect(deliveredCards.map(\.state) == [.delivered]) // A peer ACK is not chain finality.
        let deliveredReplay = try await restarted.nativeCoinage(request: request(.preparePayment(intent: intent)))
        guard case let .prepared(deliveredCard, deliveredMemo) = deliveredReplay else {
            Issue.record("Expected delivered public replay"); return
        }
        #expect(deliveredCard.state == .delivered)
        #expect(deliveredMemo == nil)
        #expect(try await restarted.nativeCoinage(request: request(.readHandoff(
            productId: intent.productId, operationId: intent.operationId
        ))) == .failed(reason: .operationNotFound))
        #expect(await harness.debits == 1)
    }

    @Test func peerAcknowledgmentRepairsLostCommitBeforePendingHandoffReplay() async throws {
        let harness = try NativeWalletHarness()
        let journal = NativeRecordMemory()
        let original = adapter(harness, journal, NativeReviewHarness())
        let intent = intent()
        _ = try await original.nativeCoinage(request: request(.preparePayment(intent: intent)))
        let restarted = adapter(harness, journal, NativeReviewHarness())
        // Host replays authenticated acknowledgments before its pending-handoff reconciliation.
        #expect(try await restarted.nativeCoinage(request: request(.noteDelivery(
            productId: intent.productId, operationId: intent.operationId
        ))) == .done)
        #expect(try await restarted.nativeCoinage(request: request(.pendingHandoffs(
            productId: intent.productId, acceptedOperations: [intent.operationId]
        ))) == .payments(payments: []))
        let replay = try await restarted.nativeCoinage(request: request(.preparePayment(intent: intent)))
        guard case let .prepared(card, memo) = replay else {
            Issue.record("Expected delivered operation replay"); return
        }
        #expect(card.state == .delivered)
        #expect(memo == nil)
        #expect(await harness.debits == 1)
    }

    @Test func mutatedOperationAndMutatedRequestIdentityCannotSpendAgain() async throws {
        let harness = try NativeWalletHarness()
        let service = adapter(harness, NativeRecordMemory(), NativeReviewHarness())
        let original = intent()
        _ = try await service.nativeCoinage(request: request(.preparePayment(intent: original)))
        var changed = original
        changed.amountCents = 2
        #expect(try await service.nativeCoinage(request: request(.preparePayment(intent: changed))) == .failed(reason: .operationConflict))
        changed = original
        changed.productId = "other.product"
        #expect(try await service.nativeCoinage(request: request(.preparePayment(intent: changed))) == .failed(reason: .operationConflict))
        changed = original
        changed.operationId = Data(repeating: 9, count: 32)
        #expect(try await service.nativeCoinage(request: request(.preparePayment(intent: changed))) == .failed(reason: .operationConflict))
        #expect(await harness.debits == 1)
    }

    @Test func denialIsDurableAndPrivacyConsentIsExplicit() async throws {
        let harness = try NativeWalletHarness(privacy: true)
        let journal = NativeRecordMemory()
        let presenter = NativeReviewHarness(approved: false)
        let first = adapter(harness, journal, presenter)
        #expect(try await first.nativeCoinage(request: request(.preparePayment(intent: intent()))) == .failed(reason: .userRejected))
        let restarted = adapter(harness, journal, NativeReviewHarness(approved: true))
        #expect(try await restarted.nativeCoinage(request: request(.preparePayment(intent: intent()))) == .failed(reason: .userRejected))
        #expect(await harness.debits == 0)
        let reviews = await presenter.reviews
        #expect(reviews.count == 1)
        #expect(reviews.first?.1 == true)
        #expect(reviews.first?.0.maxDebitCents == 1)
        #expect(reviews.first?.0.recipientIdentity == intent().peerIdentity)
    }

    @Test func logoutDuringReviewCannotDebitAndUnavailableDoesNotInspectWallet() async throws {
        let harness = try NativeWalletHarness()
        let presenter = NativeReviewHarness(suspended: true)
        let service = adapter(harness, NativeRecordMemory(), presenter)
        let task = Task { try await service.nativeCoinage(request: request(.preparePayment(intent: intent()))) }
        await presenter.waitForReview()
        service.setAvailable(false)
        service.setAvailable(true) // Same root, different activation: old review is still invalid.
        await presenter.answer(true)
        #expect(try await task.value == .failed(reason: .unavailable))
        #expect(await harness.debits == 0)
        service.setAvailable(false)
        let calls = await harness.previews
        #expect(try await service.nativeCoinage(request: request(.preparePayment(intent: intent()))) == .failed(reason: .unavailable))
        #expect(await harness.previews == calls)
    }

    @Test func wrongRootGenesisAndInstanceAreRejectedBeforeSelection() async throws {
        let harness = try NativeWalletHarness()
        let service = adapter(harness, NativeRecordMemory(), NativeReviewHarness())
        var wrong = scope
        wrong.rootPublicKey = Data(repeating: 8, count: 32)
        #expect(try await service.nativeCoinage(request: NativeCoinageRequest(scope: wrong, operation: .denomination)) == .failed(reason: .invalidRequest))
        wrong = scope
        wrong.genesisHash = Data(repeating: 8, count: 32)
        #expect(try await service.nativeCoinage(request: NativeCoinageRequest(scope: wrong, operation: .denomination)) == .failed(reason: .invalidRequest))
        wrong = scope
        wrong.coinageInstanceId = nil
        #expect(try await service.nativeCoinage(request: NativeCoinageRequest(scope: wrong, operation: .denomination)) == .failed(reason: .invalidRequest))
        #expect(await harness.previews == 0)
        #expect(await harness.debits == 0)
    }

    @Test func reconcileAndReadNeverInitializeAnApprovedButUnregisteredPlan() async throws {
        let harness = try NativeWalletHarness()
        let journal = NativeRecordMemory()
        let record = NativeCoinageOutgoing(binding: NativeCoinageBinding(scope), intent: NativeCoinageIntent(intent()),
                                           timestamp: 123, centsUnit: "1000", approval: .approved,
                                           privacyApproved: false, accepted: false, delivered: false)
        try await journal.save(.outgoing(record), authorization: {})
        let service = adapter(harness, journal, NativeReviewHarness())
        #expect(try await service.nativeCoinage(request: request(.reconcile)) == .done)
        #expect(try await service.nativeCoinage(request: request(.readHandoff(productId: "chat.product", operationId: intent().operationId))) == .failed(reason: .operationNotFound))
        #expect(await harness.previews == 0)
        #expect(await harness.debits == 0)
    }

    @Test func incomingRequiresFinalityAndPreservesMinimumAndSourceIdentityAcrossRestart() async throws {
        let harness = try NativeWalletHarness()
        let journal = NativeRecordMemory()
        let first = adapter(harness, journal, NativeReviewHarness())
        let keys = try [UInt8(10), 11].map { try SNKeyFactory().createKeypair(fromSeed: Data(repeating: $0, count: 32)).privateKey().rawData() }
        let operation = Data(repeating: 13, count: 32)
        let topup = NativeCoinageOperation.topUp(productId: "chat.product", operationId: operation,
                                                minimumAmountRaw: "2000", secretKeys: keys)
        await harness.setIncoming(.claimed(finalized: false))
        #expect(try await first.nativeCoinage(request: request(topup)) == .topUp(outcome: .pending))
        let restarted = adapter(harness, journal, NativeReviewHarness())
        await harness.setIncoming(.claimed(finalized: true))
        let reordered = NativeCoinageOperation.topUp(productId: "chat.product", operationId: operation,
                                                    minimumAmountRaw: "2000", secretKeys: Array(keys.reversed()))
        #expect(try await restarted.nativeCoinage(request: request(reordered)) == .topUp(outcome: .cleared))
        let changed = NativeCoinageOperation.topUp(productId: "chat.product", operationId: operation,
                                                  minimumAmountRaw: "1999", secretKeys: keys)
        #expect(try await restarted.nativeCoinage(request: request(changed)) == .failed(reason: .operationConflict))
        let overlap = NativeCoinageOperation.topUp(productId: "different.product", operationId: Data(repeating: 14, count: 32),
                                                  minimumAmountRaw: "1", secretKeys: [keys[0]])
        #expect(try await restarted.nativeCoinage(request: request(overlap)) == .failed(reason: .operationConflict))
        #expect(await harness.claims == 1)
    }

    @Test func incomingPartialKeepsRawPrecisionAndUnfinalizedStatusesStayPending() {
        let credited = BigUInt("340282366920938463463374607431768211")!
        #expect(TrUAPINativeCoinage.topUpOutcome(.claimedPartially(actualClaimed: credited)) == .partial(creditedAmountRaw: String(credited)))
        #expect(TrUAPINativeCoinage.topUpOutcome(.detecting) == .pending)
        #expect(TrUAPINativeCoinage.topUpOutcome(.claiming) == .pending)
        #expect(TrUAPINativeCoinage.topUpOutcome(.claimed(finalized: false)) == .pending)
        #expect(TrUAPINativeCoinage.topUpOutcome(.notClaimed) == .notClaimed)
    }

    private func adapter(_ harness: NativeWalletHarness, _ journal: NativeRecordMemory, _ presenter: NativeReviewHarness) -> TrUAPINativeCoinage {
        let service = TrUAPINativeCoinage(wallet: harness.wallet, store: journal, scope: { scope }, confirmationPresenter: presenter)
        service.setAvailable(true)
        return service
    }

    @Test func zeroMinimumClaimsAllButDoesNotSucceedBeforeNativeFinality() async throws {
        let harness = try NativeWalletHarness()
        let service = adapter(harness, NativeRecordMemory(), NativeReviewHarness())
        let key = try SNKeyFactory().createKeypair(fromSeed: Data(repeating: 55, count: 32)).privateKey().rawData()
        let operation = NativeCoinageOperation.topUp(
            productId: "chat.product", operationId: Data(repeating: 21, count: 32),
            minimumAmountRaw: "0", secretKeys: [key]
        )
        await harness.setIncoming(.claimed(finalized: false))
        #expect(try await service.nativeCoinage(request: request(operation)) == .topUp(outcome: .pending))
        await harness.setIncoming(.claimed(finalized: true))
        #expect(try await service.nativeCoinage(request: request(operation)) == .topUp(outcome: .cleared))
        #expect(await harness.claims == 1)
    }

    @Test func onlyFinalizedNativeStatusClearsTheActualMemoCoin() async throws {
        let harness = try NativeWalletHarness()
        let service = adapter(harness, NativeRecordMemory(), NativeReviewHarness())
        _ = try await service.nativeCoinage(request: request(.preparePayment(intent: intent())))
        await harness.setTransfer(.claimed(finalized: false))
        let best = try await service.nativeCoinage(request: request(.views(productId: "chat.product")))
        guard case let .payments(bestCards) = best else { Issue.record("Expected cards"); return }
        #expect(bestCards.map(\.state) == [.preparing])
        await harness.setTransfer(.claimed(finalized: true))
        let finalized = try await service.nativeCoinage(request: request(.views(productId: "chat.product")))
        guard case let .payments(finalCards) = finalized else { Issue.record("Expected cards"); return }
        #expect(finalCards.map(\.state) == [.cleared])
        #expect(await harness.debits == 1)
    }

    private func request(_ operation: NativeCoinageOperation) -> NativeCoinageRequest {
        NativeCoinageRequest(scope: scope, operation: operation)
    }

    private func intent() -> NativeCoinagePaymentIntent {
        NativeCoinagePaymentIntent(operationId: Data(repeating: 3, count: 32), productId: "chat.product", requestId: "one",
                                   peerIdentity: Data(repeating: 4, count: 32), recipientUsername: "peer", amountCents: 1)
    }
}

private actor NativeRecordMemory: NativeCoinageRecordStoring {
    private var values: [String: Data] = [:]
    func records(binding: NativeCoinageBinding) throws -> [NativeCoinageRecord] {
        try values.values.map { try JSONDecoder().decode(NativeCoinageRecord.self, from: $0) }.filter { $0.binding == binding }
    }
    func save(_ record: NativeCoinageRecord, authorization: @escaping @Sendable () throws -> Void) throws {
        try authorization()
        values[record.key] = try JSONEncoder().encode(record)
    }
}

private actor NativeWalletHarness {
    let secret: Data
    let coin: Coin
    let privacy: Bool
    private(set) var debits = 0
    private(set) var previews = 0
    private(set) var claims = 0
    private var custody: [String: TransferMemo] = [:]
    private var accepted: Set<String> = []
    private var incoming: IncomingPaymentStatus = .detecting
    private var transfer: CoinageTransferStatus = .awaitingClaim

    init(privacy: Bool = false) throws {
        let keys = try SNKeyFactory().createKeypair(fromSeed: Data(repeating: 42, count: 32))
        secret = keys.privateKey().rawData()
        coin = Coin(exponent: 0, derivationIndex: 1, age: 1, isOnchain: true, publicKey: keys.publicKey().rawData())
        self.privacy = privacy
    }

    nonisolated var wallet: TrUAPINativeCoinage.Wallet {
        TrUAPINativeCoinage.Wallet(
            denomination: { DenominationBreakdownContext(unit: 1000, precision: 5, maxExponent: 20, minExponent: -2) },
            preview: { try await self.preview($0) },
            prepare: { _, id, authorization in try await self.prepare(id, authorization: authorization) },
            retained: { await self.custody[$0] },
            statuses: { try await self.statuses($0) },
            accept: { _, _, id, _ in try await self.accept(id) },
            incomingStatus: { _, _ in await self.incoming }
        )
    }

    private func preview(_ amount: BigUInt) throws -> TransferPreview {
        previews += 1
        guard amount == 1000 else { throw CoinSelectionError.insufficientFunds }
        return TransferPreview(selectionResult: .exactMatch(coins: [coin]), fullAmount: amount,
                               scope: privacy ? .withConfirmation : .spendable)
    }
    private func prepare(_ id: String, authorization: @Sendable () throws -> Void) throws -> TransferMemo {
        if let prior = custody[id] { return prior }
        try authorization()
        debits += 1
        let memo = TransferMemo(entries: [secret], totalValue: 1000)
        custody[id] = memo
        return memo
    }
    private func statuses(_ secretKeys: [Data]) throws -> [Data: CoinageTransferState] {
        // Mirror the real service's secret-input/public-output contract; wrong or unrelated keys cannot clear.
        let publicKeys = try secretKeys.map { try SNKeyFactory().createPublicKey(fromSecret: $0).rawData() }
        return publicKeys.contains(coin.publicKey)
            ? [coin.publicKey: CoinageTransferState(coin: coin, status: transfer)]
            : [:]
    }
    private func accept(_ id: String) throws {
        guard accepted.insert(id).inserted else { throw IncomingPaymentError.alreadyExists }
        claims += 1
    }
    func setIncoming(_ value: IncomingPaymentStatus) { incoming = value }
    func setTransfer(_ value: CoinageTransferStatus) { transfer = value }
}

private actor NativeReviewHarness: TrUAPIConfirmationPresenting {
    private let approved: Bool
    private let suspended: Bool
    private var answerContinuation: CheckedContinuation<Bool, Never>?
    private var reviewWaiter: CheckedContinuation<Void, Never>?
    private(set) var reviews: [(MainPurseChatPaymentReview, Bool)] = []

    init(approved: Bool = true, suspended: Bool = false) {
        self.approved = approved
        self.suspended = suspended
    }
    func confirm(review _: UserConfirmationReview, from _: String) -> Bool { approved }
    func confirmNativeCoinage(review: MainPurseChatPaymentReview, requiresPrivacyConfirmation: Bool) async -> Bool {
        reviews.append((review, requiresPrivacyConfirmation))
        reviewWaiter?.resume()
        reviewWaiter = nil
        if suspended { return await withCheckedContinuation { answerContinuation = $0 } }
        return approved
    }
    func waitForReview() async {
        if !reviews.isEmpty { return }
        await withCheckedContinuation { reviewWaiter = $0 }
    }
    func answer(_ value: Bool) {
        answerContinuation?.resume(returning: value)
        answerContinuation = nil
    }
}
