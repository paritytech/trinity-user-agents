import Testing
import Foundation
@testable import HandoffService

/// Wire-level vectors for the RFC 0001 `VersionedUploadedFile` envelope.
/// Enum indices and byte shapes are normative and must stay compatible with other platforms.
struct VersionedUploadedFileTests {
    @Test func inlineEnvelopeMatchesSpecVector() throws {
        let fileData = Data([0xDE, 0xAD])

        let encoded = try VersionedUploadedFile.v1(.inline(fileData)).scaleEncoded()

        // version index 0, payload index 0 (inline), compact length 2, payload bytes
        #expect(encoded == Data([0x00, 0x00, 0x08, 0xDE, 0xAD]))
    }

    @Test func chunkedEnvelopeIsLegacyLayoutWithTwoBytePrefix() throws {
        let chunkedFile = ChunkedFile(
            totalSize: 300,
            chunks: [Data(repeating: 0xAB, count: 32)]
        )

        let legacyEncoding = try chunkedFile.scaleEncoded()
        let envelope = try VersionedUploadedFile.v1(.chunked(chunkedFile)).scaleEncoded()

        // version index 0, payload index 1 (chunked), then byte-for-byte the legacy layout
        #expect(envelope == Data([0x00, 0x01]) + legacyEncoding)
    }

    @Test func unsupportedVersionThrows() {
        #expect(throws: UploadedFileDecodingError.unsupportedVersion(1)) {
            try VersionedUploadedFile.scaleDecode(from: Data([0x01, 0x00]))
        }
    }

    @Test func unsupportedPayloadThrows() {
        #expect(throws: UploadedFileDecodingError.unsupportedPayload(2)) {
            try VersionedUploadedFile.scaleDecode(from: Data([0x00, 0x02]))
        }
    }
}
