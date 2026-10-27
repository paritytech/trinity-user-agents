import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

// The stored form of the last message of a room preview. The store keeps each value as one version byte, then its
// SCALE bytes. The version byte lets a later build change the stored form.

extension Chat {
    /// The version byte at the start of a stored value.
    enum AdditionStoredForm {
        static let version: UInt8 = 2
    }
}

extension ScaleEncodable {
    /// The version byte, then the SCALE bytes of the value.
    func storedBytes() throws -> Data {
        let encoder = ScaleEncoder()
        try Chat.AdditionStoredForm.version.encode(scaleEncoder: encoder)
        try encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}

extension ScaleDecodable {
    /// The value of stored bytes, or nil when the version is not known or the bytes are not a complete value.
    static func decodedIfStored(_ data: Data) -> Self? {
        guard data.first == Chat.AdditionStoredForm.version else {
            return nil
        }

        return decodedIfValid(Data(data.dropFirst()))
    }
}

// MARK: - SCALE forms

/// The host form of the last message: its content has the stored form of a message of the native chat.
extension Chat.RoomPreviewMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        status = try Chat.LocalMessage.Status.stored(scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        content = try Chat.LocalMessage.Content(scaleDecoder: scaleDecoder)
        relatedMessages = try [Chat.RelatedLocalMessage](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try UInt8(clamping: status.rawValue).encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try content.encode(scaleEncoder: scaleEncoder)
        try relatedMessages.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RelatedLocalMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        messageId = try String(scaleDecoder: scaleDecoder)
        status = try Chat.LocalMessage.Status.stored(scaleDecoder)
        timestamp = try UInt64(scaleDecoder: scaleDecoder)
        content = try Chat.LocalMessage.Content(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try messageId.encode(scaleEncoder: scaleEncoder)
        try UInt8(clamping: status.rawValue).encode(scaleEncoder: scaleEncoder)
        try timestamp.encode(scaleEncoder: scaleEncoder)
        try content.encode(scaleEncoder: scaleEncoder)
    }
}

private extension Chat.LocalMessage.Status {
    static func stored(_ scaleDecoder: any ScaleDecoding) throws -> Self {
        guard let status = try Self(rawValue: Int16(UInt8(scaleDecoder: scaleDecoder))) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        return status
    }
}
