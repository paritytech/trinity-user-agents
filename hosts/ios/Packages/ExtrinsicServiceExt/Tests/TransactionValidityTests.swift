import Testing
import SubstrateSdk
import SubstrateOperation
@testable import ExtrinsicServiceExt

@Suite("TransactionValidity")
struct TransactionValidityTests {
    @Test("mortality is expired for stale and ancientBirthBlock")
    func mortalityExpiredReasons() {
        #expect(TransactionValidity.invalid(.stale).isMortalityExpired)
        #expect(TransactionValidity.invalid(.ancientBirthBlock).isMortalityExpired)
    }

    @Test("mortality is not expired for other invalid reasons")
    func mortalityNotExpiredForOtherReasons() {
        #expect(!TransactionValidity.invalid(.future).isMortalityExpired)
        #expect(!TransactionValidity.invalid(.badProof).isMortalityExpired)
        #expect(!TransactionValidity.invalid(.exhaustsResources).isMortalityExpired)
        #expect(!TransactionValidity.invalid(.custom(7)).isMortalityExpired)
        #expect(!TransactionValidity.invalid(.fallback).isMortalityExpired)
    }

    @Test("mortality is not expired for valid and unknown")
    func mortalityNotExpiredForValidAndUnknown() {
        #expect(!TransactionValidity.valid.isMortalityExpired)
        #expect(!TransactionValidity.unknown(.cannotLookup).isMortalityExpired)
        #expect(!TransactionValidity.unknown(.fallback).isMortalityExpired)
    }

    @Test("invalid Custom decodes its string payload")
    func invalidCustomDecodes() throws {
        let json = JSON.arrayValue([.stringValue("Custom"), .stringValue("7")])

        #expect(try json.map(to: InvalidTransaction.self) == .custom(7))
    }

    @Test("unknown Custom decodes its string payload")
    func unknownCustomDecodes() throws {
        let json = JSON.arrayValue([.stringValue("Custom"), .stringValue("3")])

        #expect(try json.map(to: UnknownTransaction.self) == .custom(3))
    }

    @Test("known variants decode")
    func knownVariantsDecode() throws {
        let stale = JSON.arrayValue([.stringValue("Stale"), .null])
        let cannotLookup = JSON.arrayValue([.stringValue("CannotLookup"), .null])

        #expect(try stale.map(to: InvalidTransaction.self) == .stale)
        #expect(try cannotLookup.map(to: UnknownTransaction.self) == .cannotLookup)
    }

    @Test("a variant from a newer runtime decodes to fallback")
    func newerRuntimeVariantDecodesToFallback() throws {
        let json = JSON.arrayValue([.stringValue("SomeVariantAddedLater"), .null])

        #expect(try json.map(to: InvalidTransaction.self) == .fallback)
        #expect(try json.map(to: UnknownTransaction.self) == .fallback)
    }

    @Test("the validity error envelope decodes both branches")
    func envelopeDecodesBothBranches() throws {
        let invalid = JSON.arrayValue([
            .stringValue("Invalid"),
            .arrayValue([.stringValue("Stale"), .null])
        ])
        let unknown = JSON.arrayValue([
            .stringValue("Unknown"),
            .arrayValue([.stringValue("CannotLookup"), .null])
        ])

        #expect(try invalid.map(to: TransactionValidityError.self) == .invalid(.stale))
        #expect(try unknown.map(to: TransactionValidityError.self) == .unknown(.cannotLookup))
    }

    @Test("the validity error envelope rejects an unknown variant")
    func envelopeRejectsUnknownVariant() {
        let json = JSON.arrayValue([.stringValue("SomethingElse"), .null])

        #expect(throws: (any Error).self) {
            try json.map(to: TransactionValidityError.self)
        }
    }
}
