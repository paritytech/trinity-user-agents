import BigInt
import Coinage
import Foundation
import NovaCrypto
import TrUAPIHost

extension TrUAPINativeCoinage {
    struct TopUpRequest {
        let product: String
        let operation: Data
        let minimum: String
        let secrets: [Data]
    }

    func topUp(
        _ request: TopUpRequest,
        binding: NativeCoinageBinding,
        records: [NativeCoinageRecord],
        lease: NativeCoinageLease
    ) async throws -> NativeCoinageResponse {
        let amount = try Self.topUpAmount(request)
        let pairs = try Self.sourcePairs(request.secrets)
        let sources = pairs.map { $0.publicKey }
        let incoming = NativeCoinageIncoming(
            binding: binding,
            operation: request.operation,
            product: request.product,
            minimum: request.minimum,
            sources: sources
        )
        try await persist(incoming, records: records, binding: binding, lease: lease)
        let paymentId = "truapi-native-\(binding.key)-\(request.operation.toHex())"
        do {
            try await wallet.accept(
                amount,
                pairs.map { $0.secretKey },
                paymentId,
                request.product
            )
        } catch IncomingPaymentError.alreadyExists {
            // Only legal after our durable immutable binding matched above.
        } catch IncomingPaymentError.invalidAmount {
            throw Refusal(reason: .invalidRequest)
        } catch IncomingPaymentError.invalidSource {
            throw Refusal(reason: .invalidSource)
        } catch IncomingPaymentError.sourceBusy {
            throw Refusal(reason: .operationConflict)
        }
        try check(binding, lease)
        let status = try await wallet.incomingStatus(paymentId, request.product)
        try check(binding, lease)
        if case let .claimedPartially(actual) = status, actual >= amount {
            throw Refusal(reason: .operationConflict)
        }
        return .topUp(outcome: Self.topUpOutcome(status))
    }

    private static func topUpAmount(_ request: TopUpRequest) throws -> BigUInt {
        guard
            !request.product.isEmpty,
            request.operation.count == 32,
            let amount = BigUInt(request.minimum),
            amount.bitWidth <= 128,
            String(amount) == request.minimum
        else {
            throw Refusal(reason: .invalidRequest)
        }
        return amount
    }

    private static func sourcePairs(
        _ secrets: [Data]
    ) throws -> [(publicKey: Data, secretKey: Data)] {
        guard !secrets.isEmpty, secrets.allSatisfy({ $0.count == 64 }) else {
            throw Refusal(reason: .invalidSource)
        }
        let pairs: [(publicKey: Data, secretKey: Data)]
        do {
            pairs = try secrets
                .map {
                    (try SNKeyFactory().createPublicKey(fromSecret: $0).rawData(), $0)
                }
                .sorted {
                    $0.publicKey.lexicographicallyPrecedes($1.publicKey)
                }
        } catch {
            throw Refusal(reason: .invalidSource)
        }
        let sources = pairs.map { $0.publicKey }
        guard Set(sources).count == sources.count else {
            throw Refusal(reason: .invalidSource)
        }
        return pairs
    }

    private func persist(
        _ incoming: NativeCoinageIncoming,
        records: [NativeCoinageRecord],
        binding: NativeCoinageBinding,
        lease: NativeCoinageLease
    ) async throws {
        if let prior = records.first(where: { $0.operation == incoming.operation }) {
            guard prior == .incoming(incoming) else {
                throw Refusal(reason: .operationConflict)
            }
            return
        }
        for case let .incoming(prior) in records
            where !Set(prior.sources).isDisjoint(with: Set(incoming.sources)) {
            throw Refusal(reason: .operationConflict)
        }
        // Immutable min and source identity survive IncomingPaymentService's terminal secret cleanup.
        try await store.save(.incoming(incoming)) { [self] in try check(binding, lease) }
        try check(binding, lease)
    }
}
