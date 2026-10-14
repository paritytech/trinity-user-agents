import Foundation
import SubstrateSdk

public enum TransactionValidity: Equatable {
    case valid
    case invalid(InvalidTransaction)
    case unknown(UnknownTransaction)

    public var isMortalityExpired: Bool {
        guard case let .invalid(reason) = self else { return false }
        return reason == .stale || reason == .ancientBirthBlock
    }
}

public enum InvalidTransaction: Equatable, Decodable {
    case call
    case payment
    case future
    case stale
    case badProof
    case ancientBirthBlock
    case exhaustsResources
    case custom(UInt8)
    case badMandatory
    case mandatoryValidation
    case badSigner
    case fallback

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        let name = try container.decode(String.self)

        switch name {
        case "Call": self = .call
        case "Payment": self = .payment
        case "Future": self = .future
        case "Stale": self = .stale
        case "BadProof": self = .badProof
        case "AncientBirthBlock": self = .ancientBirthBlock
        case "ExhaustsResources": self = .exhaustsResources
        case "Custom": self = try .custom(container.decode(StringScaleMapper<UInt8>.self).value)
        case "BadMandatory": self = .badMandatory
        case "MandatoryValidation": self = .mandatoryValidation
        case "BadSigner": self = .badSigner
        default: self = .fallback
        }
    }
}

public enum UnknownTransaction: Equatable, Decodable {
    case cannotLookup
    case noUnsignedValidationFunction
    case custom(UInt8)
    case fallback

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        let name = try container.decode(String.self)

        switch name {
        case "CannotLookup": self = .cannotLookup
        case "NoUnsignedValidationFunction": self = .noUnsignedValidationFunction
        case "Custom": self = try .custom(container.decode(StringScaleMapper<UInt8>.self).value)
        default: self = .fallback
        }
    }
}
