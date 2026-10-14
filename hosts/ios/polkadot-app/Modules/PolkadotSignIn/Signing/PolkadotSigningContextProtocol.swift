import Foundation
import Products
import SubstrateSdk
import KeyDerivation

enum PolkadotSigningContextError: Error {
    case unsupportedAccountResolution
}

/// The parse inputs the signing-request parser needs: who is requesting, the
/// request model, and how to resolve the signing wallet. Shared by the full
/// interactive signing context and the confirm-only product context, so the
/// parser doesn't require a reply channel it never uses.
protocol PolkadotSigningRequestProviding: AnyObject {
    var requester: PolkadotSigningRequester { get }
    var signingModel: PolkadotHostSigningModel { get }

    func resolveWallet(for account: Any) throws -> WalletManaging
}

/// A full interactive signing context: parse inputs plus the reply channel that
/// returns a signature or a rejection to the requester.
protocol PolkadotSigningContextProtocol: PolkadotSigningRequestProviding {
    func sendResult(_ result: PolkadotHostSigningResult) async throws
    func rejectRequest() async throws
}

extension PolkadotSigningRequestProviding {
    func resolveWallet(for account: Any) throws -> WalletManaging {
        if let productAccount = account as? ProductAccountId {
            return try DynamicDerivedWallet(derivationPath: productAccount.derivationPath())
        }

        guard let accountId = account as? AccountId else {
            throw PolkadotSigningContextError.unsupportedAccountResolution
        }

        return try IdentityAccountResolver().resolveWallet(for: accountId)
    }
}
