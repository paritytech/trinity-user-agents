import Foundation
import SubstrateSdk
import Testing
@testable import Revive

struct NameHashTests {
    @Test func emptyNameReturns32ZeroBytes() throws {
        let result = try NameHash.nameHash("")
        #expect(result == Data(repeating: 0, count: 32))
    }

    @Test func namehashOfEthMatchesKnownVector() throws {
        let expected = "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
        let result = try NameHash.nameHash("eth")
        #expect(result.toHex() == expected)
    }

    @Test func namehashOfFooDotEthMatchesKnownVector() throws {
        let expected = "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f"
        let result = try NameHash.nameHash("foo.eth")
        #expect(result.toHex() == expected)
    }

    @Test func namehashOfAliceDotEthMatchesKnownVector() throws {
        let expected = "787192fc5378cc32aa956ddfdedbf26b24e8d78e40109add0eea2c1a012c3dec"
        let result = try NameHash.nameHash("alice.eth")
        #expect(result.toHex() == expected)
    }

    @Test func namehashOfDotDomainProduces32Bytes() throws {
        let result = try NameHash.nameHash("product.dot")
        #expect(result.count == 32)
    }

    @Test func namehashIsDeterministic() throws {
        let result1 = try NameHash.nameHash("test.dot")
        let result2 = try NameHash.nameHash("test.dot")
        #expect(result1 == result2)
    }

    @Test func differentNamesProduceDifferentHashes() throws {
        let result1 = try NameHash.nameHash("alice.dot")
        let result2 = try NameHash.nameHash("bob.dot")
        #expect(result1 != result2)
    }
}
