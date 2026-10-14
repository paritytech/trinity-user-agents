import Foundation
import Revive
import SubstrateSdk
import Testing
@testable import Products

struct DotNsAbiTests {
    @Test func encodeContentHashProducesCorrectSelector() throws {
        let node = try NameHash.nameHash("test.dot")
        let encoded = try DotNsAbi.encodeContentHash(node: node)

        // contenthash(bytes32) selector = 0xbc1c58d1
        #expect(encoded.prefix(4).toHex() == "bc1c58d1")
    }

    @Test func encodeContentHashIncludesNodeAsBytes32() throws {
        let node = try NameHash.nameHash("test.dot")
        let encoded = try DotNsAbi.encodeContentHash(node: node)

        // After 4-byte selector, next 32 bytes should be the node
        #expect(encoded.subdata(in: 4 ..< 36) == node)
    }

    @Test func encodeTextProducesCorrectSelector() throws {
        let node = try NameHash.nameHash("test.dot")
        let encoded = try DotNsAbi.encodeText(node: node, key: "manifest")

        // text(bytes32,string) selector = 0x59d1d43c
        #expect(encoded.prefix(4).toHex() == "59d1d43c")
    }

    @Test func encodeResolverProducesCorrectSelector() throws {
        let node = try NameHash.nameHash("test.dot")
        let encoded = try DotNsAbi.encodeResolver(node: node)

        // resolver(bytes32) selector = 0x0178b8bf
        #expect(encoded.prefix(4).toHex() == "0178b8bf")
        #expect(encoded.subdata(in: 4 ..< 36) == node)
    }

    @Test func decodeContentHashRoundTrips() throws {
        let originalHash = Data(0 ..< 34)

        let decoded = DotNsAbi.decodeContentHash(output: AbiOutput.dynamicBytes(originalHash))
        #expect(decoded == originalHash)
    }

    @Test func decodeContentHashReturnsNilForEmptyOutput() {
        let result = DotNsAbi.decodeContentHash(output: Data())
        #expect(result == nil)
    }

    @Test func decodeTextRoundTrips() throws {
        let decoded = DotNsAbi.decodeText(output: AbiOutput.string("manifest-value"))
        #expect(decoded == "manifest-value")
    }

    @Test func decodeTextReturnsNilForEmptyString() throws {
        let result = DotNsAbi.decodeText(output: AbiOutput.string(""))
        #expect(result == nil)
    }

    @Test func decodeResolverRoundTrips() throws {
        let address = Data(repeating: 0xAB, count: 20)

        #expect(DotNsAbi.decodeResolver(output: AbiOutput.address(address)) == address)
    }

    /// How the registry reports a name it holds no entry for. Passing it on as a contract address
    /// would send every subsequent read to the zero account.
    @Test func decodeResolverReturnsNilForTheZeroAddress() throws {
        #expect(DotNsAbi.decodeResolver(output: AbiOutput.address(EvmAddressFormat.zero)) == nil)
    }

    @Test func decodeResolverReturnsNilForEmptyOutput() {
        #expect(DotNsAbi.decodeResolver(output: Data()) == nil)
    }
}
