import Foundation
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("SSORawHostMessage Tests")
struct SSORawHostMessageTests {
    private func makeEnvelope(messageId: String, body: [UInt8]) throws -> Data {
        let encoder = ScaleEncoder()
        try messageId.encode(scaleEncoder: encoder)
        var data = encoder.encode()
        data.append(contentsOf: body)
        return data
    }

    @Test("Parses message ID and keeps all bytes verbatim")
    func parsesMessageIdAndKeepsBytesVerbatim() throws {
        let envelope = try makeEnvelope(messageId: "abc12345", body: [0x00, 0x01, 0x02])
        let message = try SSORawHostMessage(rawBytes: envelope)
        #expect(message.messageId == "abc12345")
        #expect(message.rawBytes == envelope)
    }

    @Test("Round-trips through OpaqueMessageWrapper")
    func scaleRoundTripThroughOpaqueWrapper() throws {
        let envelope = try makeEnvelope(messageId: "abc12345", body: [0xAA, 0xBB])
        let wrapper = try OpaqueMessageWrapper(message: SSORawHostMessage(rawBytes: envelope))
        let encoder = ScaleEncoder()
        try wrapper.encode(scaleEncoder: encoder)
        let decoded = try OpaqueMessageWrapper<SSORawHostMessage>(
            scaleDecoder: ScaleDecoder(data: encoder.encode())
        )
        #expect(decoded.message.rawBytes == envelope)
        #expect(decoded.message.messageId == "abc12345")
    }

    @Test("Rejects bytes that do not contain a decodable message ID")
    func rejectsBytesWithoutDecodableId() {
        #expect(throws: (any Error).self) {
            try SSORawHostMessage(rawBytes: Data([0xFF]))
        }
    }
}
