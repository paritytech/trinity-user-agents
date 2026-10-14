import Foundation
import SubstrateSdk
import SubstrateSdkExt

/// Wallet-independent rendering of a signing payload for display: call decoding
/// (with raw-hex fallback), call-details JSON, and the `<Bytes>`-wrapped raw
/// message. Shared by the interactive SSO signing factory and the confirm-only
/// product signing model factory so both render payloads identically.
struct PolkadotSigningCallRenderer {
    private let jsonPrinter: JSONPrettyPrinting

    init(jsonPrinter: JSONPrettyPrinting = ExtrinsicJSONProcessor()) {
        self.jsonPrinter = jsonPrinter
    }

    /// Decode call bytes for display. Falls back to the raw bytes when the
    /// coding factory is unavailable or the bytes do not decode as a known
    /// call — the decoded/raw call is only shown to the user, never signed.
    func parseCall(
        from data: Data,
        codingFactory: RuntimeCoderFactoryProtocol?
    ) -> PolkadotParsedTransactionCall {
        guard let call = codingFactory?.decodeRuntimeCall(from: data) else {
            return .raw(bytes: data)
        }

        return .callable(value: call)
    }

    /// Pretty-printed JSON for a parsed call: the callable's SCALE-compatible
    /// JSON, or the raw bytes as a hex string.
    func callDetailsText(
        _ call: PolkadotParsedTransactionCall,
        codingFactory: RuntimeCoderFactoryProtocol?
    ) throws -> String {
        let callJSON: JSON
        switch call {
        case let .raw(bytes):
            callJSON = .stringValue(bytes.toHex(includePrefix: true))
        case let .callable(value):
            guard let codingFactory else {
                throw PolkadotSigningError.missingRuntimeProvider
            }
            callJSON = try value.toScaleCompatibleJSON(
                with: codingFactory.createRuntimeJsonContext().toRawContext()
            )
        }

        return try jsonPrinter.prettyPrintedString(from: callJSON)
    }

    /// Wrap raw bytes in `<Bytes></Bytes>` per the Polkadot raw-signing convention.
    func wrappedBytes(_ data: Data) throws -> Data {
        try makeWrappedMessage(message: data)
    }

    /// Serialize a raw string payload (hex or UTF-8) and wrap it for signing.
    func wrappedBytes(fromString string: String) throws -> Data {
        let message = try makeSerializedMessage(string: string)
        return try makeWrappedMessage(message: message)
    }
}

private extension PolkadotSigningCallRenderer {
    func makeSerializedMessage(string: String) throws -> Data {
        guard !string.isHex() else {
            return try Data(hexString: string)
        }
        guard let data = string.data(using: .utf8) else {
            throw PolkadotSigningError.rawDataCorrupted
        }
        return data
    }

    func makeWrappedMessage(message: Data) throws -> Data {
        let prefix = "<Bytes>"
        let suffix = "</Bytes>"

        guard
            let suffixData = suffix.data(using: .ascii),
            let prefixData = prefix.data(using: .ascii)
        else {
            throw PolkadotSigningError.rawDataCorrupted
        }

        if message.prefix(prefixData.count) == prefixData,
           message.suffix(suffixData.count) == suffixData {
            return message
        }

        return prefixData + message + suffixData
    }
}
