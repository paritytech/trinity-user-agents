import Foundation
import Revive

public enum DotNsAbi {
    // contenthash(bytes32 node) returns (bytes)
    private static let contenthashFunction = EvmAbi.Function(
        name: "contenthash",
        inputs: [.bytes(length: 32)],
        outputs: [.dynamicBytes]
    )

    // text(bytes32 node, string key) returns (string)
    private static let textFunction = EvmAbi.Function(
        name: "text",
        inputs: [.bytes(length: 32), .string],
        outputs: [.string]
    )

    // resolver(bytes32 node) returns (address) — on the name registry, not a resolver.
    private static let resolverFunction = EvmAbi.Function(
        name: "resolver",
        inputs: [.bytes(length: 32)],
        outputs: [.address]
    )

    public static func encodeResolver(node: Data) throws -> Data {
        try EvmAbi.encode(resolverFunction, parameters: [node])
    }

    /// Nil when the registry has no entry for the name, which it reports as the zero address.
    public static func decodeResolver(output: Data) -> EvmAddress? {
        guard
            let decoded = try? EvmAbi.decode(resolverFunction, output: output),
            let bytes = decoded.first as? Data,
            let address = try? EvmAddressFormat.validate(bytes),
            address != EvmAddressFormat.zero
        else {
            return nil
        }

        return address
    }

    public static func encodeContentHash(node: Data) throws -> Data {
        try EvmAbi.encode(contenthashFunction, parameters: [node])
    }

    public static func decodeContentHash(output: Data) -> Data? {
        guard let decoded = try? EvmAbi.decode(contenthashFunction, output: output) else { return nil }
        return decoded.first as? Data
    }

    public static func encodeText(node: Data, key: String) throws -> Data {
        try EvmAbi.encode(textFunction, parameters: [node, key])
    }

    public static func decodeText(output: Data) -> String? {
        guard let decoded = try? EvmAbi.decode(textFunction, output: output) else { return nil }
        let result = decoded.first as? String
        return result?.isEmpty == true ? nil : result
    }
}
