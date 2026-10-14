import Foundation
import BandersnatchApi
import KeyDerivation

final class MockBandersnatchKeyManager: BandersnatchKeyManaging {
    let publicKey = Data.random(of: 33)!
    let proof = Data.random(of: 785)!
    let alias = Data.random(of: 32)!

    func getRawPublicKey() throws -> Data { publicKey }

    func sign(_: Data) throws -> Data { Data() }

    func createProof(
        _: Data,
        members _: [BandersnatchPubKey],
        context _: Data,
        domainSize _: BandersnatchApi.RingDomainSize
    ) throws -> Data {
        proof
    }

    func deriveAlias(for _: Data) throws -> Data { alias }
}
