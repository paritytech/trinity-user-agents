import Foundation
import ExtrinsicService
import SubstrateSdk
import KeyDerivation
@testable import Coinage

/// An origin factory that hands back ``StubExtrinsicOrigin`` for every request. Pass `errorToThrow` to
/// make the unload-token path fail.
final class StubOriginFactory: OriginCreating, @unchecked Sendable {
    let errorToThrow: Error?
    private(set) var signedOriginChainIds: [ChainId] = []

    init(errorToThrow: Error? = nil) {
        self.errorToThrow = errorToThrow
    }

    func createAsCoinOrigin(for _: WalletManaging) throws -> ExtrinsicOriginDefining {
        StubExtrinsicOrigin()
    }

    func createSignedOrigin(for _: WalletManaging, chainId: ChainId) async throws -> ExtrinsicOriginDefining {
        signedOriginChainIds.append(chainId)
        return StubExtrinsicOrigin()
    }

    func createInfallibleUnpaidSignedOrigin(for _: WalletManaging) throws -> ExtrinsicOriginDefining {
        StubExtrinsicOrigin()
    }

    func createAsUnloadTokenOrigins(
        voucherGroups: [[Voucher]],
        currentDate _: Date,
        blockHash _: BlockHashData?
    ) async throws -> [ExtrinsicOriginDefining] {
        if let errorToThrow { throw errorToThrow }
        return voucherGroups.map { _ in StubExtrinsicOrigin() }
    }
}
