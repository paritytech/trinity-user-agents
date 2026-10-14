import Foundation
import NovaCrypto
import SDKLogger
import SubstrateSdk

public protocol StatementStoreProofVerifying {
    func verifyProof(for statement: Statement) -> Bool
}

public final class StatementStoreProofVerifier {
    let signatureVerifier: SNSignatureVerifierProtocol
    let logger: SDKLoggerProtocol?

    public init(logger: SDKLoggerProtocol?) {
        signatureVerifier = SNSignatureVerifier()
        self.logger = logger
    }
}

extension StatementStoreProofVerifier: StatementStoreProofVerifying {
    public func verifyProof(for statement: Statement) -> Bool {
        switch statement.getProof() {
        case let .sr25519(signature, signer):
            return verify(signatureData: signature, signerData: signer, from: statement)
        case .none:
            logger?.warning("No proof found")
            return false
        }
    }
}

private extension StatementStoreProofVerifier {
    func verify(signatureData: Data, signerData: Data, from statement: Statement) -> Bool {
        do {
            let proofData = try statement.deriveProofData()
            let signature = try SNSignature(rawData: signatureData)
            let publicKey = try SNPublicKey(rawData: signerData)

            return signatureVerifier.verify(
                signature,
                forOriginalData: proofData,
                using: publicKey
            )
        } catch {
            logger?.error("Verification failed \(error)")
            return false
        }
    }
}
