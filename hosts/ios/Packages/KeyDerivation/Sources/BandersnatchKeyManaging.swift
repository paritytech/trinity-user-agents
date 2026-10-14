import Foundation
import Keystore_iOS
import BandersnatchApi
import SubstrateSdk

public typealias BandersnatchPubKey = Data

public protocol BandersnatchKeyManaging: RawPublicKeyProviding, RawKeypairSigning {
    func createProof(
        _ message: Data,
        members: [BandersnatchPubKey],
        context: Data,
        domainSize: BandersnatchApi.RingDomainSize
    ) throws -> Data

    func deriveAlias(for context: Data) throws -> Data
}

public extension BandersnatchKeyManaging {
    func getMemberKey() throws -> BandersnatchPubKey {
        try getRawPublicKey()
    }
}

public protocol BandersnatchEntropyDeriving {
    func deriveEntropy(from seed: Data) throws -> Data
}

public final class BandersnatchKeyManager {
    let entropyManager: RootEntropyManaging
    let entropyDeriver: BandersnatchEntropyDeriving

    private var memberKey: BandersnatchPubKey?

    public init(
        entropyDeriver: BandersnatchEntropyDeriving,
        entropyManager: RootEntropyManaging
    ) {
        self.entropyDeriver = entropyDeriver
        self.entropyManager = entropyManager
    }
}

private extension BandersnatchKeyManager {
    func getEntropy() throws -> Data {
        let seed = try entropyManager.fetchRootEntropy()
        return try entropyDeriver.deriveEntropy(from: seed)
    }
}

extension BandersnatchKeyManager: BandersnatchKeyManaging {
    public func getRawPublicKey() throws -> BandersnatchPubKey {
        if let memberKey {
            return memberKey
        }

        let entropy = try getEntropy()
        let memberKey = try BandersnatchApi.deriveMemberKey(from: entropy)
        self.memberKey = memberKey

        return memberKey
    }

    public func sign(_ data: Data) throws -> Data {
        let entropy = try getEntropy()
        return try BandersnatchApi.sign(entropy: entropy, message: data)
    }

    public func createProof(
        _ message: Data,
        members: [BandersnatchPubKey],
        context: Data,
        domainSize: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        let entropy = try getEntropy()
        return try BandersnatchApi.createProof(
            from: entropy,
            members: members,
            message: message,
            context: context,
            domainSize: domainSize
        )
    }

    public func deriveAlias(for context: Data) throws -> Data {
        let entropy = try getEntropy()

        return try BandersnatchApi.deriveAlias(fromEntropy: entropy, context: context)
    }
}
