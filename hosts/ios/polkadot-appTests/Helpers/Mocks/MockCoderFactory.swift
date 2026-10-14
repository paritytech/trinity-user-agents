import Foundation
import SubstrateSdk

final class MockCoderFactory: RuntimeCoderFactoryProtocol {
    var specVersion: UInt32 = 1
    var txVersion: UInt32 = 1
    var metadata: RuntimeMetadataProtocol { fatalError("not needed") }

    func createEncoder() -> DynamicScaleEncoding { fatalError("not needed") }
    func createDecoder(from _: Data) throws -> DynamicScaleDecoding { MockDecoder() }
    func createRuntimeJsonContext() -> RuntimeJsonContext { RuntimeJsonContext(prefersRawAddress: false) }
    func hasType(for _: String) -> Bool { false }
    func getTypeNode(for _: String) -> Node? { nil }
    func getCall(for _: CallCodingPath) -> CallMetadata? { nil }
    func getConstant(for _: ConstantCodingPath) -> ModuleConstantMetadata? { nil }
}

private final class MockDecoder: DynamicScaleDecoding {
    var remained: Int { 0 }
    func read(type _: String) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readOption(type _: String) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readVector(type _: String) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readCompact(type _: String) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readFixedArray(type _: String, length _: UInt64) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readBytes(length _: Int) throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readString() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU8() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU16() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU32() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU64() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU128() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readU256() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI8() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI16() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI32() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI64() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI128() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readI256() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func readBool() throws -> JSON { throw NSError(domain: "mock", code: 0) }
    func read<T: ScaleCodable>() throws -> T { throw NSError(domain: "mock", code: 0) }
}
