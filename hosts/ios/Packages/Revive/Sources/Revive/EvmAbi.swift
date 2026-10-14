import Foundation
import Web3Core

public enum EvmAbiError: Error, Equatable {
    case parameterCountMismatch(function: String)
    case invalidAddress(Data)
    case encodingFailed(function: String)
    case decodingFailed(function: String)
}

/// Solidity ABI encoding of contract calls and their answers. Consumers describe each function with
/// ``Function`` and exchange plain Swift values, so web3swift is linked here only.
public enum EvmAbi {
    public indirect enum ParameterType: Equatable, Sendable {
        case address
        case string
        case bytes(length: UInt64)
        case dynamicBytes
        /// A dynamic-length array of `element`.
        case array(ParameterType)
    }

    public struct Function: Sendable {
        public let name: String
        public let inputs: [ParameterType]
        public let outputs: [ParameterType]

        public init(name: String, inputs: [ParameterType], outputs: [ParameterType]) {
            self.name = name
            self.inputs = inputs
            self.outputs = outputs
        }
    }

    /// The selector followed by `parameters` in declaration order: an ``EvmAddress`` for `.address`,
    /// `Data` for bytes, `String` for `.string`, an array of those for `.array`.
    public static func encode(_ function: Function, parameters: [Any]) throws -> Data {
        guard parameters.count == function.inputs.count else {
            throw EvmAbiError.parameterCountMismatch(function: function.name)
        }

        let values = try zip(function.inputs, parameters).map { type, value in
            try Self.encodable(value, as: type)
        }

        guard let encoded = ABI.Element.function(function.element).encodeParameters(values) else {
            throw EvmAbiError.encodingFailed(function: function.name)
        }

        return encoded
    }

    /// The returned values in declaration order, in the same Swift types ``encode(_:parameters:)`` takes.
    public static func decode(_ function: Function, output: Data) throws -> [Any] {
        guard let decoded = ABI.Element.function(function.element).decodeReturnData(output) else {
            throw EvmAbiError.decodingFailed(function: function.name)
        }

        return try function.outputs.enumerated().map { index, type in
            guard let value = decoded[String(index)] else {
                throw EvmAbiError.decodingFailed(function: function.name)
            }

            return Self.decodable(value, as: type)
        }
    }
}

private extension EvmAbi {
    static func encodable(_ value: Any, as type: ParameterType) throws -> Any {
        switch type {
        case .address:
            guard let bytes = value as? Data, let address = EthereumAddress(bytes) else {
                throw EvmAbiError.invalidAddress(value as? Data ?? Data())
            }
            return address
        case let .array(element):
            guard let values = value as? [Any] else { return value }
            return try values.map { try encodable($0, as: element) }
        case .string,
             .bytes,
             .dynamicBytes:
            return value
        }
    }

    static func decodable(_ value: Any, as type: ParameterType) -> Any {
        switch type {
        case .address:
            return (value as? EthereumAddress)?.addressData ?? value
        case let .array(element):
            guard let values = value as? [Any] else { return value }
            return values.map { decodable($0, as: element) }
        case .string,
             .bytes,
             .dynamicBytes:
            return value
        }
    }
}

private extension EvmAbi.Function {
    var element: ABI.Element.Function {
        ABI.Element.Function(
            name: name,
            inputs: inputs.map { ABI.Element.InOut(name: "", type: $0.element) },
            outputs: outputs.map { ABI.Element.InOut(name: "", type: $0.element) },
            constant: false,
            payable: false
        )
    }
}

private extension EvmAbi.ParameterType {
    var element: ABI.Element.ParameterType {
        switch self {
        case .address: .address
        case .string: .string
        case let .bytes(length): .bytes(length: length)
        case .dynamicBytes: .dynamicBytes
        case let .array(element): .array(type: element.element, length: 0)
        }
    }
}
