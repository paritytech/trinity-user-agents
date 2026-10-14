import Foundation
import NovaCrypto
import SubstrateSdk

extension SNPublicKey: @retroactive AccountProtocol, @retroactive SignerProviding {
    public var accountId: AccountId {
        rawData()
    }

    public var publicKey: Data {
        rawData()
    }

    public var signatureFormat: ExtrinsicSignatureFormat {
        .substrate
    }

    public var signaturePayloadFormat: ExtrinsicSignaturePayloadFormat {
        .regular
    }

    public var signatureType: CryptoType { .sr25519 }

    public func toAddress() throws -> AccountAddress {
        try SS58AddressFactory().address(fromAccountId: rawData(), type: 42)
    }

    public var account: AccountProtocol? { self }
}
