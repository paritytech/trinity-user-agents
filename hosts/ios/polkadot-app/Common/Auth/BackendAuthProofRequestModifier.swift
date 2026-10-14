import Foundation
import KeyDerivation
import SubstrateSdk
import UniqueDevice

enum BackendAuthProofModifierError: Error {
    case missingChallengeHeader
    case malformedChallengeHeader
    case unexpectedSignatureType
}

final class BackendAuthProofRequestModifier: HttpRequestModifier {
    private let inner: HttpRequestModifier
    private let wallet: WalletManaging

    init(
        inner: HttpRequestModifier,
        wallet: WalletManaging
    ) {
        self.inner = inner
        self.wallet = wallet
    }

    func visit(request: inout URLRequest) throws {
        try inner.visit(request: &request)

        guard let challengeB64 = request.value(forHTTPHeaderField: "Auth-Challenge") else {
            throw BackendAuthProofModifierError.missingChallengeHeader
        }
        guard let challenge = Data(base64Encoded: challengeB64) else {
            throw BackendAuthProofModifierError.malformedChallengeHeader
        }

        let clientId = try wallet.getRawPublicKey()
        let body = request.httpBody ?? Data()
        let payload = (challenge + clientId + body.sha256()).sha256()

        guard case let .sr25519(signature) = try wallet.sign(data: payload) else {
            throw BackendAuthProofModifierError.unexpectedSignatureType
        }

        request.setValue(clientId.base64EncodedString(), forHTTPHeaderField: "Auth-ClientId")
        request.setValue(signature.base64EncodedString(), forHTTPHeaderField: "Auth-ClientProof")
    }
}
