import Foundation
import SubstrateSdk

/// Keeps every `append(json:type:)` in order; the caller encodes nothing else, so any other entry point
/// is a test failure.
final class RecordingScaleEncoder: DynamicScaleEncoding {
    struct Entry: Equatable {
        let json: JSON
        let type: String
    }

    private(set) var entries: [Entry] = []

    func append(json: JSON, type: String) throws {
        entries.append(Entry(json: json, type: type))
    }

    func appendOption(json _: JSON, type _: String) throws { throw StubError.unused }
    func appendVector(json _: JSON, type _: String) throws { throw StubError.unused }
    func appendCompact(json _: JSON, type _: String) throws { throw StubError.unused }
    func appendFixedArray(json _: JSON, type _: String) throws { throw StubError.unused }
    func appendBytes(json _: JSON) throws { throw StubError.unused }
    func appendRawData(_: Data) throws { throw StubError.unused }
    func appendCommonOption(isNull _: Bool) throws { throw StubError.unused }
    func appendString(json _: JSON) throws { throw StubError.unused }
    func appendU8(json _: JSON) throws { throw StubError.unused }
    func appendU16(json _: JSON) throws { throw StubError.unused }
    func appendU32(json _: JSON) throws { throw StubError.unused }
    func appendU64(json _: JSON) throws { throw StubError.unused }
    func appendU128(json _: JSON) throws { throw StubError.unused }
    func appendU256(json _: JSON) throws { throw StubError.unused }
    func appendI8(json _: JSON) throws { throw StubError.unused }
    func appendI16(json _: JSON) throws { throw StubError.unused }
    func appendI32(json _: JSON) throws { throw StubError.unused }
    func appendI64(json _: JSON) throws { throw StubError.unused }
    func appendI128(json _: JSON) throws { throw StubError.unused }
    func appendI256(json _: JSON) throws { throw StubError.unused }
    func appendBool(json _: JSON) throws { throw StubError.unused }
    func append(encodable _: some ScaleCodable) throws { throw StubError.unused }
    func newEncoder() -> DynamicScaleEncoding { RecordingScaleEncoder() }
    func canEncodeOptional(for _: String) -> Bool { false }
    func encode() throws -> Data { Data() }
}
