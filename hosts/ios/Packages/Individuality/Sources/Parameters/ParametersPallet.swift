import Foundation
import SubstrateSdk
import SubstrateSdkExt

/// An unset parameter has no storage entry; the runtime then applies a default that no metadata exposes.
public enum ParametersPallet {
    public static let name = "Parameters"

    public enum Storage {
        case parameters
    }

    public enum StatementStorageParameter: String {
        case notificationAllowance = "NotificationAllowance"
    }

    /// `RuntimeParametersKey::StatementStorage(ParametersKey::<name>(<name>))`; the unit key encodes as `{}`.
    public struct StatementStorageKey: Encodable {
        public let parameter: StatementStorageParameter

        public init(parameter: StatementStorageParameter) {
            self.parameter = parameter
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()
            try container.encode("StatementStorage")

            var inner = container.nestedUnkeyedContainer()
            try inner.encode(parameter.rawValue)
            try inner.encode([String: JSON]())
        }
    }

    public struct StatementStorageValue<Value: Decodable>: Decodable {
        public let parameter: String
        public let value: Value

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let group = try container.decode(String.self)

            guard group == "StatementStorage" else {
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unexpected parameters group: \(group)"
                )
            }

            var inner = try container.nestedUnkeyedContainer()
            parameter = try inner.decode(String.self)
            value = try inner.decode(Value.self)
        }
    }
}

extension ParametersPallet.Storage: StoragePathConvertible {
    public var moduleName: String { ParametersPallet.name }

    public var name: String {
        switch self {
        case .parameters:
            "Parameters"
        }
    }
}
