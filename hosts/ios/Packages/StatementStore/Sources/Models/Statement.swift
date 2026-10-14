import Foundation
import SubstrateSdk

public typealias Statement = [StatementField]

public extension Statement {
    func getScaleEncodedPayload() -> Data? {
        compactMap { $0.getScaleEncodedPayload() }.first
    }

    func getTopic1() -> Data? {
        compactMap { $0.getTopic1() }.first
    }

    func getTopic2() -> Data? {
        compactMap { $0.getTopic2() }.first
    }

    func getTopic3() -> Data? {
        compactMap { $0.getTopic3() }.first
    }

    func getTopic4() -> Data? {
        compactMap { $0.getTopic4() }.first
    }

    func getProof() -> StatementProof? {
        compactMap { $0.getProof() }.first
    }

    func getSenderAccountId() -> Data? {
        guard case let .sr25519(_, signer) = getProof() else {
            return nil
        }
        return signer
    }

    func getExpiry() -> UInt64? {
        compactMap { $0.getExpiry() }.first
    }

    func getChannel() -> Data? {
        compactMap { $0.getChannel() }.first
    }

    func deriveProofData() throws -> Data {
        let fields = filter { field in
            switch field {
            case .proof:
                false
            default:
                true
            }
        }.sortedByIndex()

        let encoder = ScaleEncoder()

        try fields.forEach { field in
            try field.encode(scaleEncoder: encoder)
        }

        return encoder.encode()
    }
}
