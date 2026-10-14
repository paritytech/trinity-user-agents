@preconcurrency import KeyDerivation
import SubstrateSdk
import NovaCrypto

public protocol VoucherKeyDeriving: VoucherKeypairFactoryProtocol, Sendable {
    /// Creates a key manager for a voucher index to perform Bandersnatch operations (proofs, signing, aliases).
    func createKeyManager(index: CoinageKeyIndex) throws -> any BandersnatchKeyManaging
}

public extension VoucherKeyDeriving {
    /// Convenience over ``createKeyManager(index:)`` for a model whose index it reads.
    func createKeyManager(for model: Voucher) throws -> any BandersnatchKeyManaging {
        try createKeyManager(index: model.derivationIndex)
    }
}

enum VoucherEntropyDerivingError: Error {
    case invalidDerivationPath
}

public final class VoucherKeypairFactory {
    let entropyManager: RootEntropyManaging

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension VoucherKeypairFactory: VoucherKeyDeriving {
    public func derivePublicKey(index: CoinageKeyIndex) throws -> PublicKey {
        try createKeyManager(index: index).getMemberKey()
    }

    public func createKeyManager(index: CoinageKeyIndex) throws -> any BandersnatchKeyManaging {
        BandersnatchKeyManager(
            entropyDeriver: VoucherEntropyDeriving(path: voucherPath(for: index)),
            entropyManager: entropyManager
        )
    }
}

public extension VoucherKeypairFactory {
    func voucherPath(for derivationIndex: CoinageKeyIndex) -> String {
        let purse = CoinageConstants.Derivation.mainPurse
        let page = derivationIndex.installation.pageSegment

        return "//coinage-ring-vrf//\(purse)//\(page)//\(derivationIndex.item)"
    }
}

// MARK: -

final class VoucherEntropyDeriving: BandersnatchEntropyDeriving {
    private let path: String
    private lazy var junctionFactory: JunctionFactoryProtocol = SubstrateJunctionFactory()

    init(path: String) {
        self.path = path
    }

    /// Ring-VRF keys derive entirely from entropy — there is no public (soft) derivation — so every
    /// junction in `//coinage-ring-vrf//<purse>//<page>//<item>` must be hard and is folded into the
    /// entropy chain. A soft junction has no ring-VRF meaning and is rejected.
    func deriveEntropy(from seed: Data) throws -> Data {
        let junctionResult = try junctionFactory.parse(path: path)
        let chaincodes = junctionResult.chaincodes
        guard !chaincodes.contains(where: { $0.type != .hard }) else {
            throw VoucherEntropyDerivingError.invalidDerivationPath
        }

        return try chaincodes.reduce(seed) { partialResult, chainCode in
            try partialResult.blake2b32WithKey(chainCode.data)
        }
    }
}
