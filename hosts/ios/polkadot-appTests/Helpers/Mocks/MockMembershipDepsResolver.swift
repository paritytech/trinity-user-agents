import Foundation
import Individuality
import SubstrateSdk

final class MockMembershipDepsResolver: MembershipDepsResolving {
    var deps: MembershipDeps?
    var isValidRing = true
    var membersPalletIndex: UInt8? = 50
    private(set) var recordedGenesisHash: ChainId?
    private(set) var recordedPalletIndex: UInt8?
    private(set) var recordedValidationGenesisHash: ChainId?
    private(set) var recordedValidationPalletIndex: UInt8?

    func resolve(genesisHash: ChainId, palletIndex: UInt8?) async throws -> MembershipDeps? {
        recordedGenesisHash = genesisHash
        recordedPalletIndex = palletIndex
        return deps
    }

    func validate(genesisHash: ChainId, palletIndex: UInt8?) async throws -> Bool {
        recordedValidationGenesisHash = genesisHash
        recordedValidationPalletIndex = palletIndex
        return isValidRing
    }

    func membersPalletIndex(genesisHash _: ChainId) async throws -> UInt8? {
        membersPalletIndex
    }
}
