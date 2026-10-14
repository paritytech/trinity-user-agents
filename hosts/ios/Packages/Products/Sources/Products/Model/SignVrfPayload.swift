import Foundation
import KeyDerivation
import SubstrateSdk

public struct SignVrfPayload: Equatable, Codable {
    /// RFC-0023 transcript bounds a host must enforce against hostile callers,
    /// checked before any user confirmation.
    public static let maxTranscriptItems = 32
    public static let maxTranscriptBytes = 8_192

    public let account: ProductAccountId
    @HexCodable public var transcriptLabel: Data
    public let items: [VrfTranscriptItem]

    public init(account: ProductAccountId, transcriptLabel: Data, items: [VrfTranscriptItem]) {
        self.account = account
        _transcriptLabel = HexCodable(wrappedValue: transcriptLabel)
        self.items = items
    }

    public func validateTranscriptSize() throws {
        guard items.count <= Self.maxTranscriptItems else {
            throw SignVrfError.transcriptTooLarge(
                "VRF transcript has \(items.count) items, at most \(Self.maxTranscriptItems) are allowed"
            )
        }

        let totalSize = items.reduce(transcriptLabel.count) { size, item in
            size + item.label.count + item.value.count
        }

        guard totalSize <= Self.maxTranscriptBytes else {
            throw SignVrfError.transcriptTooLarge(
                "VRF transcript is \(totalSize) bytes, at most \(Self.maxTranscriptBytes) are allowed"
            )
        }
    }
}

/// Wire form of ``VrfSignature`` returned to product scripts; bytes encode as hex.
public struct SignVrfResult: Codable {
    @HexCodable public var preOutput: Data
    @HexCodable public var proof: Data

    public init(signature: VrfSignature) {
        _preOutput = HexCodable(wrappedValue: signature.preOutput)
        _proof = HexCodable(wrappedValue: signature.proof)
    }
}
