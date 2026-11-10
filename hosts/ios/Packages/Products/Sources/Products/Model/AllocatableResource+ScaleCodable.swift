import Foundation
import SubstrateSdk

// MARK: - AllocatableResource + ScaleCodable

extension AllocatableResource: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            self = .statementStoreAllowance
        case 1:
            self = .bulletInAllowance
        case 2:
            let dest = try ProductAccountSelector(scaleDecoder: scaleDecoder)
            self = .smartContractAllowance(dest: dest)
        case 3:
            self = .autoSigning
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case .statementStoreAllowance,
             .bulletInAllowance,
             .autoSigning:
            break
        case let .smartContractAllowance(dest):
            try dest.encode(scaleEncoder: scaleEncoder)
        }
    }

    private var scaleIndex: UInt8 {
        switch self {
        case .statementStoreAllowance: 0
        case .bulletInAllowance: 1
        case .smartContractAllowance: 2
        case .autoSigning: 3
        }
    }
}

// MARK: - AllocationOutcome + ScaleCodable

extension AllocationOutcome: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            let resource = try AllocatedResource(scaleDecoder: scaleDecoder)
            self = .allocated(resource)
        case 1:
            self = .rejected
        case 2:
            self = .notAvailable
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .allocated(resource):
            try resource.encode(scaleEncoder: scaleEncoder)
        case .rejected,
             .notAvailable:
            break
        }
    }

    private var scaleIndex: UInt8 {
        switch self {
        case .allocated: 0
        case .rejected: 1
        case .notAvailable: 2
        }
    }
}

// MARK: - AllocatedResource + ScaleCodable

extension AllocatedResource: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case 0:
            let key = try Data(scaleDecoder: scaleDecoder)
            self = .statementStoreAllowance(privateKey: key)
        case 1:
            let key = try Data(scaleDecoder: scaleDecoder)
            self = .bulletInAllowance(privateKey: key)
        case 2:
            self = .smartContractAllowance
        case 3:
            let secrets = try AutoSigningSecrets(scaleDecoder: scaleDecoder)
            self = .autoSigning(secrets)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try scaleIndex.encode(scaleEncoder: scaleEncoder)

        switch self {
        case let .statementStoreAllowance(key):
            try key.encode(scaleEncoder: scaleEncoder)
        case let .bulletInAllowance(key):
            try key.encode(scaleEncoder: scaleEncoder)
        case .smartContractAllowance:
            break
        case let .autoSigning(secrets):
            try secrets.encode(scaleEncoder: scaleEncoder)
        }
    }

    private var scaleIndex: UInt8 {
        switch self {
        case .statementStoreAllowance: 0
        case .bulletInAllowance: 1
        case .smartContractAllowance: 2
        case .autoSigning: 3
        }
    }
}

// MARK: - AutoSigningSecrets + ScaleCodable

extension AutoSigningSecrets: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        // `Sr25519SecretKey` is `[u8; 64]` on the wire — raw bytes, no length prefix.
        let key = try scaleDecoder.readAndConfirm(count: Self.privateKeyLength)
        try self.init(productRootPrivateKey: key)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        // Length invariant enforced by the memberwise init.
        scaleEncoder.appendRaw(data: productRootPrivateKey)
    }
}
