import Foundation
import Products
import Testing

@testable import polkadot_app

@Suite("RingVrfError Wrapping Tests")
struct RingVrfErrorWrappingTests {
    struct CodedStubError: HostCallCodedError {
        let code: String
        let message: String
    }

    struct PlainStubError: Error {}

    @Test("RingVrfError passes through unchanged")
    func ringVrfErrorPassthrough() {
        #expect(RingVrfError.wrapping(RingVrfError.rejected) == .rejected)
    }

    @Test(
        "CreateProofError maps case by case",
        arguments: [
            (CreateProofError.ringNotFound, RingVrfError.ringNotFound),
            (CreateProofError.notMember, RingVrfError.notMember),
            (CreateProofError.keyNotRegistered, RingVrfError.keyNotRegistered),
            (CreateProofError.keyNotInRing, RingVrfError.keyNotInRing),
            (CreateProofError.notAllowlisted, RingVrfError.notAllowlisted),
            (CreateProofError.rejected, RingVrfError.rejected),
            (CreateProofError.unknown("boom"), RingVrfError.unknown("boom"))
        ]
    )
    func createProofErrorMapping(source: CreateProofError, expected: RingVrfError) {
        #expect(RingVrfError.wrapping(source) == expected)
    }

    @Test(
        "GetAliasError maps case by case",
        arguments: [
            (GetAliasError.ringNotFound, RingVrfError.ringNotFound),
            (GetAliasError.notMember, RingVrfError.notMember),
            (GetAliasError.keyNotRegistered, RingVrfError.keyNotRegistered),
            (GetAliasError.keyNotInRing, RingVrfError.keyNotInRing),
            (GetAliasError.rejected, RingVrfError.rejected),
            (GetAliasError.unknown("boom"), RingVrfError.unknown("boom"))
        ]
    )
    func getAliasErrorMapping(source: GetAliasError, expected: RingVrfError) {
        #expect(RingVrfError.wrapping(source) == expected)
    }

    @Test("Other coded errors collapse to unknown with the coded message")
    func codedErrorFallback() {
        let error = CodedStubError(code: "SomethingElse", message: "coded failure")

        #expect(RingVrfError.wrapping(error) == .unknown("coded failure"))
    }

    @Test("Non-coded errors collapse to unknown with the localized description")
    func plainErrorFallback() {
        let error = PlainStubError()

        #expect(RingVrfError.wrapping(error) == .unknown(error.localizedDescription))
    }
}
