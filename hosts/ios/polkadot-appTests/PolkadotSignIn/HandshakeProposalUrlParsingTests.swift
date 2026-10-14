import Foundation
import SubstrateSdk
import Testing

@testable import polkadot_app

@Suite("HandshakeProposal URL Parsing Tests")
struct HandshakeProposalUrlParsingTests {
    private let activeScheme = DeeplinkTestSchemes.active
    private let otherScheme = DeeplinkTestSchemes.other

    @Test("Parses pairing url with the active build scheme")
    func parsesActiveSchemeUrl() throws {
        let url = try makePairingUrl(scheme: activeScheme)

        let proposal = try #require(HandshakeProposal(url: url))

        guard case let .v1(data) = proposal else {
            Issue.record("Expected v1 proposal")
            return
        }
        #expect(data.metadata == "Test Host")
    }

    @Test("Rejects pairing url with the other build flavor scheme")
    func rejectsOtherSchemeUrl() throws {
        let url = try makePairingUrl(scheme: otherScheme)

        #expect(HandshakeProposal(url: url) == nil)
    }

    @Test("Rejects url with unexpected host")
    func rejectsUnexpectedHost() throws {
        let url = try makePairingUrl(scheme: activeScheme, host: "unpair")

        #expect(HandshakeProposal(url: url) == nil)
    }
}

private extension HandshakeProposalUrlParsingTests {
    func makePairingUrl(scheme: String, host: String = "pair") throws -> URL {
        let handshakeHex = try makeV1ProposalData().toHex(includePrefix: true)
        let urlString = "\(scheme)://\(host)?handshake=\(handshakeHex)"
        return try #require(URL(string: urlString))
    }

    func makeV1ProposalData() throws -> Data {
        let encoder = ScaleEncoder()
        try UInt8(0).encode(scaleEncoder: encoder)
        try encoder.appendRaw(data: Data.randomOrError(of: 32))
        try encoder.appendRaw(data: Data.randomOrError(of: 32))
        try "Test Host".encode(scaleEncoder: encoder)
        try ScaleOption<String>.none.encode(scaleEncoder: encoder)
        try ScaleOption<String>.none.encode(scaleEncoder: encoder)
        try ScaleOption<String>.none.encode(scaleEncoder: encoder)
        return encoder.encode()
    }
}
