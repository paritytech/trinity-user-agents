import Foundation
import SubstrateSdk

/// One SSO remote message kept as raw wire bytes. Decodes only the envelope's
/// leading message id (for dedup and delivery correlation); the body stays
/// opaque so request types unknown to this app version still round-trip to
/// the Rust core.
struct SSORawHostMessage {
    let messageId: String
    let rawBytes: Data

    init(rawBytes: Data) throws {
        let decoder = try ScaleDecoder(data: rawBytes)
        messageId = try String(scaleDecoder: decoder)
        self.rawBytes = rawBytes
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
