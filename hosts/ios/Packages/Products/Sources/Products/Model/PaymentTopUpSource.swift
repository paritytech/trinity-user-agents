import Foundation
import SubstrateSdk

public enum PaymentTopUpSource: Equatable, Hashable {
    case productAccount(derivationIndex: ProductAccountSelector)
    case privateKey(Data)
    /// Bearer coins identified by their sr25519 secret keys (W3S receive path).
    case coins(secretKeys: [Data])
}

public extension PaymentTopUpSource {
    enum DecodingError: Error, LocalizedError {
        case unknownTag(String)

        public var errorDescription: String? {
            switch self {
            case let .unknownTag(tag):
                "unknown paymentTopUp sourceTag: \(tag)"
            }
        }
    }
}

extension PaymentTopUpSource: Decodable {
    private enum CodingKeys: String, CodingKey {
        case sourceTag
        case sourceDerivationIndex
        case sourceKeyHex
        case sourceKeyListHex
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let tag = try container.decode(String.self, forKey: .sourceTag)
        switch tag {
        case "ProductAccount":
            let selector = try container.decode(ProductAccountSelector.self, forKey: .sourceDerivationIndex)
            self = .productAccount(derivationIndex: selector)
        case "PrivateKey":
            let hex = try container.decode(String.self, forKey: .sourceKeyHex)
            self = try .privateKey(Data(hexString: hex))
        case "Coins":
            let hexList = try container.decode([String].self, forKey: .sourceKeyListHex)
            let secretKeys = try hexList.map { try Data(hexString: $0) }
            self = .coins(secretKeys: secretKeys)
        default:
            throw DecodingError.unknownTag(tag)
        }
    }
}
