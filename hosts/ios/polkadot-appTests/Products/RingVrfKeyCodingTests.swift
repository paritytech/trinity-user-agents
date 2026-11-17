import struct TrUAPIHost.ProductId
import Foundation
import Products
import SubstrateSdk
import Testing

@Suite("RingVrfKey Coding Tests")
struct RingVrfKeyCodingTests {
    @Test("Disclosure decodes from the host-api Status strings")
    func disclosureDecodesFromStatusStrings() throws {
        let decoded = try JSONDecoder().decode(
            [RingVrfKeyDisclosure].self,
            from: Data(#"["Anonymized","PublicKey"]"#.utf8)
        )

        #expect(decoded == [.anonymized, .publicKey])
    }

    @Test("Anonymized entry encodes handle as tuple and public key as null")
    func anonymizedEntryShape() throws {
        let collectionId = Data(repeating: 0x07, count: 32)
        let entry = RegisteredRingVrfKey(
            handle: RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(1)),
            rings: [
                RingLocation(
                    chainId: Data(repeating: 0x01, count: 32),
                    junctions: [.palletInstance(50), .collectionId(collectionId)]
                )
            ],
            publicKey: nil
        )

        let json = try entry.toScaleCompatibleJSON()

        #expect(json.handle?.arrayValue?.first?.stringValue == "peopl.paseo")
        #expect(json.rings?.arrayValue?.count == 1)
        #expect(json.publicKey == nil || json.publicKey == .null)
    }

    @Test("Entry round-trips through JSON with a public key")
    func entryRoundTripWithPublicKey() throws {
        let entry = RegisteredRingVrfKey(
            handle: RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(0)),
            rings: [RingLocation(chainId: Data(repeating: 0x02, count: 32), junctions: [.palletInstance(50)])],
            publicKey: Data(repeating: 0xAB, count: 32)
        )

        let data = try JSONEncoder().encode(entry)
        let decoded = try JSONDecoder().decode(RegisteredRingVrfKey.self, from: data)

        #expect(decoded == entry)
    }

    @Test("Handle encodes as the [productId, index] tuple")
    func handleTupleShape() throws {
        let handle = RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(1))

        let data = try JSONEncoder().encode(handle)

        #expect(String(decoding: data, as: UTF8.self) == #"["peopl.paseo",1]"#)
        #expect(try JSONDecoder().decode(RingVrfKeyHandle.self, from: data) == handle)
    }

    @Test("Entry decodes when publicKey is null")
    func entryDecodesNullPublicKey() throws {
        let json = #"{"handle":["peopl.paseo",1],"rings":[],"publicKey":null}"#

        let decoded = try JSONDecoder().decode(RegisteredRingVrfKey.self, from: Data(json.utf8))

        #expect(decoded.handle == RingVrfKeyHandle(productId: try! ProductId("peopl.paseo"), index: .index(1)))
        #expect(decoded.publicKey == nil)
    }

    @Test("New error cases expose the host-api codes")
    func errorCodes() {
        #expect(CreateProofError.keyNotRegistered.code == "KeyNotRegistered")
        #expect(CreateProofError.keyNotInRing.code == "KeyNotInRing")
        #expect(CreateProofError.notAllowlisted.code == "NotAllowlisted")
        #expect(GetAliasError.keyNotRegistered.code == "KeyNotRegistered")
        #expect(GetAliasError.keyNotInRing.code == "KeyNotInRing")
        #expect(ListRingVrfKeysError.notConnected.code == "NotConnected")
        #expect(ListRingVrfKeysError.rejected.code == "Rejected")
    }
}
