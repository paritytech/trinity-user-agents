import Foundation
import SubstrateSdk
import NovaCrypto

public final class StatementStoreKeypairSigner {
    let signer: SNSignerProtocol
    let signingAccount: AccountId

    public init(keypair: SNKeypairProtocol) {
        signer = SNSigner(keypair: keypair)
        signingAccount = keypair.publicKey().rawData()
    }
}

extension StatementStoreKeypairSigner: StatementStoreSigning {
    public var accountId: AccountId {
        signingAccount
    }

    public func sign(_ data: Data) throws -> StatementProof {
        let signature = try signer.sign(data).rawData()

        return .sr25519(signature: signature, signer: signingAccount)
    }
}
