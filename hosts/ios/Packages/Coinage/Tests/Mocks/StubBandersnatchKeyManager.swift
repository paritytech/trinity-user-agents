import Foundation
import KeyDerivation
import BandersnatchApi

/// A Bandersnatch key manager returning fixed placeholder bytes for every operation.
final class StubBandersnatchKeyManager: BandersnatchKeyManaging {
    func getRawPublicKey() throws -> Data { Data(repeating: 0, count: 32) }
    func sign(_: Data) throws -> Data { Data(repeating: 0, count: 32) }

    func createProof(
        _: Data,
        members _: [BandersnatchPubKey],
        context _: Data,
        domainSize _: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        Data(repeating: 0, count: 64)
    }

    func deriveAlias(for _: Data) throws -> Data { Data(repeating: 0, count: 32) }
}
