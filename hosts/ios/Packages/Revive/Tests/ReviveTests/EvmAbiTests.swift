import Foundation
import SubstrateSdk
import Testing
@testable import Revive

/// Vectors pinned against ethers v6 encoding the `AccountDataStore` and DotNs resolver ABIs.
struct EvmAbiTests {
    private static let register = EvmAbi.Function(
        name: "registerCoinageInstallation",
        inputs: [.dynamicBytes],
        outputs: []
    )
    private static let get = EvmAbi.Function(
        name: "getCoinageInstallations",
        inputs: [.address],
        outputs: [.array(.dynamicBytes)]
    )
    private static let text = EvmAbi.Function(name: "text", inputs: [.bytes(length: 32), .string], outputs: [.string])
    private static let resolver = EvmAbi.Function(name: "resolver", inputs: [.bytes(length: 32)], outputs: [.address])

    private static let record = "0xb3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e80acfc55b8155"
        + "9e511662b8d79151ac504ca50de705fe6736054d586"
    private static let owner = "0x1111111111111111111111111111111111111111"

    private static let registerCall = "0xe561868d"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "000000000000000000000000000000000000000000000000000000000000003c"
        + "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e"
        + "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000"

    private static let getCall = "0x740204c60000000000000000000000001111111111111111111111111111111111111111"

    private static let returnedList = "0x"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "0000000000000000000000000000000000000000000000000000000000000002"
        + "0000000000000000000000000000000000000000000000000000000000000040"
        + "00000000000000000000000000000000000000000000000000000000000000a0"
        + "000000000000000000000000000000000000000000000000000000000000003c"
        + "b3ee8017efe8450090d9237a0f28236d6b9f3ffbd0629e41fa89da7ce1577e9e"
        + "80acfc55b81559e511662b8d79151ac504ca50de705fe6736054d58600000000"
        + "0000000000000000000000000000000000000000000000000000000000000002"
        + "0102000000000000000000000000000000000000000000000000000000000000"

    private static let returnedAddress = "0x000000000000000000000000abababababababababababababababababababab"

    private static let returnedText = "0x"
        + "0000000000000000000000000000000000000000000000000000000000000020"
        + "000000000000000000000000000000000000000000000000000000000000000e"
        + "6d616e69666573742d76616c7565000000000000000000000000000000000000"

    @Test("dynamic bytes are encoded after the selector")
    func encodeDynamicBytes() throws {
        let encoded = try EvmAbi.encode(Self.register, parameters: [Data(hexString: Self.record)])

        #expect(encoded.toHex(includePrefix: true) == Self.registerCall)
    }

    @Test("an address parameter is taken as its twenty bytes")
    func encodeAddress() throws {
        let encoded = try EvmAbi.encode(Self.get, parameters: [Data(hexString: Self.owner)])

        #expect(encoded.toHex(includePrefix: true) == Self.getCall)
    }

    @Test("bytes32 and string parameters select text(bytes32,string)")
    func encodeBytes32AndString() throws {
        let encoded = try EvmAbi.encode(Self.text, parameters: [Data(repeating: 0x01, count: 32), "manifest"])

        #expect(encoded.prefix(4).toHex() == "59d1d43c")
        #expect(encoded.subdata(in: 4 ..< 36) == Data(repeating: 0x01, count: 32))
    }

    @Test("an address of the wrong size is refused before encoding")
    func encodeInvalidAddress() {
        #expect(throws: EvmAbiError.invalidAddress(Data([0x01]))) {
            try EvmAbi.encode(Self.get, parameters: [Data([0x01])])
        }
    }

    @Test("a parameter list that does not match the signature is refused")
    func encodeCountMismatch() {
        #expect(throws: EvmAbiError.parameterCountMismatch(function: "text")) {
            try EvmAbi.encode(Self.text, parameters: [Data(repeating: 0x01, count: 32)])
        }
    }

    @Test("a returned array of dynamic bytes decodes in order")
    func decodeArray() throws {
        let decoded = try EvmAbi.decode(Self.get, output: Data(hexString: Self.returnedList))

        #expect(try decoded.first as? [Data] == [Data(hexString: Self.record), Data([0x01, 0x02])])
    }

    @Test("a returned address decodes to its twenty bytes")
    func decodeAddress() throws {
        let decoded = try EvmAbi.decode(Self.resolver, output: Data(hexString: Self.returnedAddress))

        #expect(decoded.first as? Data == Data(repeating: 0xAB, count: 20))
    }

    @Test("a returned string decodes as text")
    func decodeString() throws {
        let decoded = try EvmAbi.decode(Self.text, output: Data(hexString: Self.returnedText))

        #expect(decoded.first as? String == "manifest-value")
    }

    @Test("an answer that does not fit the signature fails rather than decoding to nothing")
    func decodeGarbage() {
        #expect(throws: EvmAbiError.decodingFailed(function: "getCoinageInstallations")) {
            try EvmAbi.decode(Self.get, output: Data([0x01, 0x02, 0x03]))
        }
    }
}
