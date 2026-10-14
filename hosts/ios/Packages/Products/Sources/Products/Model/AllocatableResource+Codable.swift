import Foundation
import SubstrateSdk

extension AllocatableResource: Decodable {
    enum CodingKeys: String, CodingKey {
        case kind
        case dest
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(String.self, forKey: .kind)

        switch kind {
        case "StatementStoreAllowance":
            self = .statementStoreAllowance
        case "BulletinAllowance":
            self = .bulletInAllowance
        case "SmartContractAllowance":
            let dest = try container.decode(ProductAccountSelector.self, forKey: .dest)
            self = .smartContractAllowance(dest: dest)
        case "AutoSigning":
            self = .autoSigning
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .kind,
                in: container,
                debugDescription: "Unknown AllocatableResource kind: \(kind)"
            )
        }
    }
}

// MARK: - AllocationOutcome + Encodable

extension AllocationOutcome: Encodable {
    enum CodingKeys: String, CodingKey {
        case kind
        case autoSigningSecrets
        case statementStoreAllowance
        case bulletInAllowance
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        switch self {
        case let .allocated(resource):
            try container.encode("Allocated", forKey: .kind)
            switch resource {
            case let .autoSigning(secrets):
                try container.encode(secrets, forKey: .autoSigningSecrets)
            case let .statementStoreAllowance(privateKey):
                let account = SlotAccount(slotAccountKey: privateKey)
                try container.encode(account, forKey: .statementStoreAllowance)
            case let .bulletInAllowance(privateKey):
                let account = SlotAccount(slotAccountKey: privateKey)
                try container.encode(account, forKey: .bulletInAllowance)
            case .smartContractAllowance:
                break
            }
        case .rejected:
            try container.encode("Rejected", forKey: .kind)
        case .notAvailable:
            try container.encode("NotAvailable", forKey: .kind)
        }
    }
}

// MARK: - AutoSigningSecrets + Encodable

extension AutoSigningSecrets: Encodable {
    enum CodingKeys: String, CodingKey {
        case productRootPrivateKey
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(
            productRootPrivateKey.toHex(includePrefix: true),
            forKey: .productRootPrivateKey
        )
    }
}

private struct SlotAccount: Encodable {
    @HexCodable var slotAccountKey: Data
}
