import Foundation
import Products
import SubstrateSdk
import KeyDerivation

final class ProductSigningContext: PolkadotSigningContextProtocol {
    let requester: PolkadotSigningRequester
    let signingModel: PolkadotHostSigningModel

    private var signResultContinuation: CheckedContinuation<SignResult, Error>?
    private var createTransactionContinuation: CheckedContinuation<CreateTransactionResult, Error>?

    init(
        requesterName: String,
        signingModel: PolkadotHostSigningModel
    ) {
        requester = PolkadotSigningRequester(name: requesterName, iconUrl: nil)
        self.signingModel = signingModel
    }

    func setSignResultContinuation(_ continuation: CheckedContinuation<SignResult, Error>) {
        signResultContinuation = continuation
    }

    func setCreateTransactionContinuation(
        _ continuation: CheckedContinuation<CreateTransactionResult, Error>
    ) {
        createTransactionContinuation = continuation
    }

    func sendResult(_ result: PolkadotHostSigningResult) async throws {
        switch result {
        case let .signedPayload(signature, signedTransaction):
            guard let continuation = signResultContinuation else { return }
            continuation.resume(returning: SignResult(
                signature: signature.toHex(includePrefix: true),
                signedTx: signedTransaction?.toHex(includePrefix: true)
            ))
            signResultContinuation = nil

        case let .rawSignature(signature):
            guard let continuation = signResultContinuation else { return }
            continuation.resume(returning: SignResult(
                signature: signature.toHex(includePrefix: true),
                signedTx: nil
            ))
            signResultContinuation = nil

        case let .signedTransaction(encodedTransaction):
            guard let continuation = createTransactionContinuation else { return }
            continuation.resume(returning: CreateTransactionResult(
                signedTransaction: encodedTransaction.toHex(includePrefix: true)
            ))
            createTransactionContinuation = nil
        }
    }

    func rejectRequest() async throws {
        if let continuation = signResultContinuation {
            continuation.resume(throwing: ProductNativeApiError.signingRejected)
            signResultContinuation = nil
        } else if let continuation = createTransactionContinuation {
            continuation.resume(throwing: ProductNativeApiError.signingRejected)
            createTransactionContinuation = nil
        }
    }
}
