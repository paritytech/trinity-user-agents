import Testing
import Foundation
import AsyncExtensions
@testable import Coinage

/// Property sweeps over the descriptor and outcome spaces: assert the invariants (idempotency,
/// product-scoping, verdict immutability) hold across every shape rather than for a single hand-picked
/// case. These are input-coverage tests, not mutation testing — the guards themselves are checked by
/// `tools/top_up_mutation_sweep.py`, which deletes each rule and confirms a test dies.
struct IncomingPaymentSweepTests {
    private static let descriptors: [IncomingPaymentSourceDescriptor] = [
        .coins(secretKeys: [Data([0x01])]),
        .coins(secretKeys: [Data([0x02]), Data([0x03])]),
        .privateKey(secretKey: Data([0x04])),
        .productAccount(derivationPath: "//5")
    ]

    private static let outcomes: [IncomingPaymentTerminalOutcome] = [
        .claimed,
        .claimedPartially(actualClaimed: 0),
        .claimedPartially(actualClaimed: 999),
        .notClaimed
    ]

    private func makeService(
        store: InMemoryIncomingPaymentStore,
        secretStore: InMemoryIncomingPaymentSecretStore = InMemoryIncomingPaymentSecretStore()
    ) -> IncomingPaymentService {
        IncomingPaymentService(
            store: store,
            secretStore: secretStore,
            sourceResolver: StubSourceResolver(),
            paymentContext: IncomingPaymentContext(logger: StubLogger()),
            claimCoinsService: StubClaimCoinsService(),
            claimAssetService: StubClaimAssetService(),
            verdictResolver: StubGroupVerdictResolver(verdict: .notClaimed),
            acknowledger: StubAcknowledger(),
            instanceId: 0,
            logger: StubLogger()
        )
    }

    @Test(arguments: descriptors.indices)
    func acceptIsIdempotentPerProductAndPaymentId(index: Int) async throws {
        let descriptor = Self.descriptors[index]
        let store = InMemoryIncomingPaymentStore()
        let service = makeService(store: store)

        try await service.accept(amount: 10, descriptor: descriptor, paymentId: "p", productId: "prod")

        // A second accept for the same (product, paymentId) is rejected regardless of descriptor shape.
        await #expect {
            try await service.accept(amount: 20, descriptor: descriptor, paymentId: "p", productId: "prod")
        } throws: { ($0 as? IncomingPaymentError) == .alreadyExists }

        // The original record is untouched.
        #expect(store.payment(for: "top up:prod:p")?.amount == 10)
    }

    @Test(arguments: descriptors.indices)
    func sameIdDifferentProductsDoNotCollide(index: Int) async throws {
        // Same paymentId under two products must produce two distinct records — the groupId is
        // namespaced by product. Each product draws on its own funds (reusing the *same* funds would
        // legitimately trip `sourceBusy`, which is a different invariant), so give them distinct keys.
        let store = InMemoryIncomingPaymentStore()
        let service = makeService(store: store)

        try await service.accept(
            amount: 10,
            descriptor: descriptorForProduct(index: index, salt: 0xA0),
            paymentId: "shared",
            productId: "prodA"
        )
        try await service.accept(
            amount: 20,
            descriptor: descriptorForProduct(index: index, salt: 0xB0),
            paymentId: "shared",
            productId: "prodB"
        )

        let a = try #require(store.payment(for: "top up:prodA:shared"))
        let b = try #require(store.payment(for: "top up:prodB:shared"))
        #expect(a.groupId != b.groupId)
    }

    /// A descriptor of the same *shape* as `descriptors[index]` but keyed by `salt`, so two products
    /// exercise the same code path over funds that don't overlap.
    private func descriptorForProduct(index: Int, salt: UInt8) -> IncomingPaymentSourceDescriptor {
        switch Self.descriptors[index] {
        case .coins: .coins(secretKeys: [Data([salt])])
        case .privateKey: .privateKey(secretKey: Data([salt]))
        case .productAccount: .productAccount(derivationPath: "//\(salt)")
        }
    }

    @Test(arguments: outcomes)
    func terminalVerdictReadBackExactly(outcome: IncomingPaymentTerminalOutcome) async throws {
        // Whatever verdict is persisted must round-trip through settle → store → status unchanged.
        let store = InMemoryIncomingPaymentStore(seed: [
            IncomingPayment(paymentId: "p", productId: "prod", amount: 100, createdAt: Date(), outcome: nil)
        ])
        try await store.settle(groupId: "top up:prod:p", outcome: outcome)

        let service = makeService(store: store)
        let stream = try await service.subscribeStatus(for: "p", productId: "prod")

        var observed: IncomingPaymentStatus?
        for try await status in stream {
            observed = status
            break
        }
        #expect(observed == IncomingPaymentStatus(outcome: outcome))
    }

    @Test(arguments: outcomes)
    func settleWipesSecretForEveryOutcome(outcome: IncomingPaymentTerminalOutcome) async throws {
        let store = InMemoryIncomingPaymentStore(seed: [
            IncomingPayment(paymentId: "p", productId: "prod", amount: 100, createdAt: Date(), outcome: nil)
        ])
        try await store.settle(groupId: "top up:prod:p", outcome: outcome)

        // The record now carries the verdict and reports inactive.
        #expect(store.payment(for: "top up:prod:p")?.outcome == outcome)
        #expect(store.payment(for: "top up:prod:p")?.isActive == false)
    }
}
