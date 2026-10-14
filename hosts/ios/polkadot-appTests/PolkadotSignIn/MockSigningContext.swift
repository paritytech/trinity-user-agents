import Foundation
import Products
import SubstrateSdk
import KeyDerivation

@testable import polkadot_app

final class MockSigningContext: PolkadotSigningContextProtocol {
    let requester: PolkadotSigningRequester
    let signingModel: PolkadotHostSigningModel

    private let wallet: WalletManaging?
    private let resolveError: Error?

    init(
        requester: PolkadotSigningRequester,
        signingModel: PolkadotHostSigningModel,
        wallet: WalletManaging
    ) {
        self.requester = requester
        self.signingModel = signingModel
        self.wallet = wallet
        resolveError = nil
    }

    init(
        requester: PolkadotSigningRequester,
        signingModel: PolkadotHostSigningModel,
        resolveError: Error
    ) {
        self.requester = requester
        self.signingModel = signingModel
        wallet = nil
        self.resolveError = resolveError
    }

    func resolveWallet(for _: Any) throws -> WalletManaging {
        if let resolveError {
            throw resolveError
        }
        return wallet!
    }

    func sendResult(_: PolkadotHostSigningResult) async throws {}
    func rejectRequest() async throws {}
}
