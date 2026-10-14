import Foundation
import KeyDerivation
import NovaCrypto

/// A resolved source ready to sign or be claimed. Rebuilt from the descriptor on each run, so a
/// resumed top-up produces exactly the keypairs the first attempt would have.
public enum ResolvedIncomingSource {
    /// An external-asset holder to onboard vouchers from (product-account or private-key sources).
    case wallet(any WalletManaging)
    /// Bearer coin secret keys to claim (coins source).
    case coins(secretKeys: [Data])
}

public enum IncomingPaymentSourceResolverError: Error, Equatable {
    /// A coins source names no coins: nothing could ever be claimed from it.
    case emptyCoinKeys
    /// A coin key that does not derive a public key could never be claimed.
    case invalidCoinKey
}

/// Turns a stored source descriptor into signing/claim material.
public protocol IncomingPaymentSourceResolving: Sendable {
    func resolve(descriptor: IncomingPaymentSourceDescriptor) async throws -> ResolvedIncomingSource
}

public final class IncomingPaymentSourceResolver: IncomingPaymentSourceResolving, @unchecked Sendable {
    private let entropyManager: RootEntropyManaging
    private let snKeyFactory: any SNKeyFactoryProtocol

    public init(entropyManager: RootEntropyManaging, snKeyFactory: any SNKeyFactoryProtocol) {
        self.entropyManager = entropyManager
        self.snKeyFactory = snKeyFactory
    }

    public func resolve(descriptor: IncomingPaymentSourceDescriptor) async throws -> ResolvedIncomingSource {
        switch descriptor {
        case let .productAccount(derivationPath):
            let wallet = DynamicDerivedWallet(
                derivationPath: derivationPath,
                entropyManager: entropyManager
            )
            _ = try wallet.getRawPublicKey()
            return .wallet(wallet)

        case let .privateKey(secretKey):
            let wallet = DynamicDerivedWallet(secretKeyProvider: { secretKey })
            _ = try wallet.getRawPublicKey()
            return .wallet(wallet)

        case let .coins(secretKeys):
            try validateCoinKeys(secretKeys)
            return .coins(secretKeys: secretKeys)
        }
    }
}

private extension IncomingPaymentSourceResolver {
    /// Every coin key must derive a public key now: a claim cannot be built from one that does not,
    /// and `accept` must refuse the source rather than store a secret that fails later as notClaimed.
    func validateCoinKeys(_ secretKeys: [Data]) throws {
        guard !secretKeys.isEmpty else {
            throw IncomingPaymentSourceResolverError.emptyCoinKeys
        }
        for secretKey in secretKeys {
            guard (try? snKeyFactory.createPublicKey(fromSecret: secretKey)) != nil else {
                throw IncomingPaymentSourceResolverError.invalidCoinKey
            }
        }
    }
}
