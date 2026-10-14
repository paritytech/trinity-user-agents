import AsyncExtensions
import DurableTransactions
import ExtrinsicService
import Foundation
@preconcurrency import NovaCrypto

/// A claim of a coin a peer handed us, rebuilt into the coin its first attempt recorded.
///
/// Minting into the *same* coin is what lets a payment we already made out of it keep waiting on it: a
/// claim retried into a fresh coin would leave that payment waiting for ever on one that will never
/// exist. Signed with the peer's key from the policy's params, which only the payment message carries.
struct ClaimRebuild: CoinageRebuild {
    struct Claim: Sendable {
        let receivedKey: Data
        let source: PublicKey
        let destination: PublicKey
    }

    let coinQuery: any CoinOnChainQuerying
    let builder: ClaimExtrinsicBuilder
    let snKeyFactory: any SNKeyFactoryProtocol

    /// A claim's failures are always worth weighing: the peer's coin is money nothing else will
    /// collect, so only the window ends its rebuilds.
    func terms(of params: Data) -> RebuildTerms? {
        guard let claim = try? CoinageSubmissionParams.decodeClaim(params) else { return nil }

        return RebuildTerms(deadline: claim.retryUntil, retriesFailures: true)
    }

    func resolve(
        _ transactions: [ScheduledDurableTx],
        assets: [CoinageTxId: CoinageTxEntry]
    ) async throws -> [CoinageTxId: Claim] {
        transactions.reduce(into: [:]) { resolved, transaction in
            guard let destination = assets[transaction.id]?.outputs.first?.publicKey,
                  let params = try? CoinageSubmissionParams.decodeClaim(transaction.policy.params),
                  let source = try? snKeyFactory.createPublicKey(fromSecret: params.receivedKey).rawData()
            else {
                return
            }

            resolved[transaction.id] = Claim(
                receivedKey: params.receivedKey,
                source: source,
                destination: destination
            )
        }
    }

    func inputs(of transaction: Claim) -> Set<PublicKey> {
        [transaction.source]
    }

    func presence(of inputs: Set<PublicKey>) async throws -> AnyAsyncSequence<Set<PublicKey>> {
        coinPresence(of: inputs, reading: coinQuery)
    }

    func build(_ transactions: [Claim]) async throws -> [ExtrinsicBuiltModel] {
        try await builder.build(transactions.map { ($0.receivedKey, $0.destination) })
    }
}
