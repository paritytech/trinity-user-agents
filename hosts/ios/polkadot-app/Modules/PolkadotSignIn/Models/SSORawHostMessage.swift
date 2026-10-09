import Foundation
import SubstrateSdk

/// One SSO remote message kept as raw wire bytes. Decodes only the envelope's
/// leading message id (for dedup and delivery correlation); the body stays
/// opaque so request types unknown to this app version still round-trip to
/// the Rust core. A `Cancel` is also recognised, so it can reach the core
/// ahead of the request it withdraws.
struct SSORawHostMessage {
    let messageId: String
    let rawBytes: Data

    /// The request a `Cancel` withdraws; `nil` for every other message.
    let withdrawnMessageId: String?

    init(rawBytes: Data) throws {
        let decoder = try ScaleDecoder(data: rawBytes)
        messageId = try String(scaleDecoder: decoder)
        withdrawnMessageId = Self.decodeWithdrawnMessageId(from: decoder)
        self.rawBytes = rawBytes
    }
}

private extension SSORawHostMessage {
    static let versionOneIndex: UInt8 = 0
    static let cancelIndex: UInt8 = 24

    static func decodeWithdrawnMessageId(from decoder: ScaleDecoder) -> String? {
        guard
            let version = try? UInt8(scaleDecoder: decoder), version == versionOneIndex,
            let variant = try? UInt8(scaleDecoder: decoder), variant == cancelIndex,
            let withdrawnMessageId = try? String(scaleDecoder: decoder) else {
            return nil
        }

        return withdrawnMessageId
    }
}

extension SSORawHostMessage: HostMessageIdentifiable {}

extension SSORawHostMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let bytes = try scaleDecoder.readAndConfirm(count: scaleDecoder.remained)
        try self.init(rawBytes: bytes)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: rawBytes)
    }
}

extension SSORawHostMessage: Equatable {}

typealias OpaqueSSORawHostMessage = OpaqueMessageWrapper<SSORawHostMessage>
