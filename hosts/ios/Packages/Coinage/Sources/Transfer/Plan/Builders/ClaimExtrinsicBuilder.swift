import DurableTransactions
import ExtrinsicService
import Foundation
import KeyDerivation
import SDKLogger
import SubstrateSdk

/// Declares a claim of a coin a peer handed us: a `Coinage.transfer` of that coin into one of ours,
/// signed by the peer's key.
///
/// Shared by the claim path, which declares it once, and by ``ClaimRebuild``, which declares it again
/// into the coin the first attempt recorded. Minting into the same coin is what lets a payment we have
/// already made out of it keep waiting on it.
struct ClaimExtrinsicBuilder: Sendable {
    /// The origin factory is a shared, stateless service, but its protocol cannot carry `Sendable`: the
    /// app's conformer inherits from a base class, which Swift forbids a `Sendable` class from doing. The
    /// reference is only read here, so it is vouched for at the property rather than for the whole type.
    nonisolated(unsafe) let originFactory: OriginCreating
    let factory: any DurableTxMaking
    let chainId: ChainId

    func request(receivedKey: Data, destination: PublicKey) throws -> DurableTxRequest {
        let call = CoinagePallet.Calls.Transfer(to: destination)
        let wallet = DynamicDerivedWallet(secretKeyProvider: { receivedKey })

        return try DurableTxRequest(
            builder: { try $0.adding(call: call.callAsFunction()) },
            origin: originFactory.createAsCoinOrigin(for: wallet)
        )
    }

    func build(_ claims: [(receivedKey: Data, destination: PublicKey)]) async throws -> [ExtrinsicBuiltModel] {
        try await factory.makeExtrinsics(
            claims.map { try request(receivedKey: $0.receivedKey, destination: $0.destination) },
            chainId: chainId
        )
    }
}
