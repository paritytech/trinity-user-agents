@testable import polkadot_app
import Foundation
import Testing

enum SdpCoderTests {
    // MARK: - Test Data

    static let validOfferSdp = """
    v=0
    o=- 1234567890 2 IN IP4 0.0.0.0
    s=-
    t=0 0
    m=application 9 UDP/DTLS/SCTP webrtc-datachannel
    c=IN IP4 0.0.0.0
    a=ice-ufrag:testUfrag
    a=ice-pwd:testPassword123
    a=fingerprint:sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99
    a=setup:actpass
    a=mid:0
    a=sctp-port:5000

    """

    static let validAnswerSdp = """
    v=0
    o=- 9876543210 1 IN IP4 0.0.0.0
    s=-
    t=0 0
    m=application 9 UDP/DTLS/SCTP webrtc-datachannel
    c=IN IP4 0.0.0.0
    a=ice-ufrag:answerUfrag
    a=ice-pwd:answerPassword456
    a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
    a=setup:active
    a=mid:0
    a=sctp-port:5000

    """

    static func makeIPv4Candidate(
        foundation: String = "1",
        priority: UInt32 = 2_130_706_431,
        transport: String = "UDP",
        ip: String = "192.168.1.100",
        port: UInt16 = 54_321,
        type: String = "host"
    ) -> PeerConnectionCandidate {
        let sdp = "candidate:\(foundation) 1 \(transport) \(priority) \(ip) \(port) typ \(type)"
        return PeerConnectionCandidate(sdp: sdp, sdpMLineIndex: 0, sdpMid: "0")
    }

    static func makeIPv6Candidate(
        foundation: String = "2",
        priority: UInt32 = 2_130_706_430,
        transport: String = "UDP",
        ip: String = "2001:db8::1",
        port: UInt16 = 12_345,
        type: String = "host"
    ) -> PeerConnectionCandidate {
        let sdp = "candidate:\(foundation) 1 \(transport) \(priority) \(ip) \(port) typ \(type)"
        return PeerConnectionCandidate(sdp: sdp, sdpMLineIndex: 0, sdpMid: "0")
    }
}

// MARK: - Setup Encoding/Decoding Tests

extension SdpCoderTests {
    struct SetupEncodingTests {
        let sut = SdpCoder()

        @Test("Encodes and decodes offer SDP without candidates")
        func encodeDecodeOfferWithoutCandidates() throws {
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: []
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(input == decoded)
        }

        @Test("Encodes and decodes answer SDP without candidates")
        func encodeDecodeAnswerWithoutCandidates() throws {
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validAnswerSdp,
                candidates: []
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(decoded == input)
        }

        @Test("Encodes and decodes SDP with IPv4 candidate")
        func encodeDecodeSdpWithIPv4Candidate() throws {
            let candidate = SdpCoderTests.makeIPv4Candidate()
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: [candidate]
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(decoded == input)
        }

        @Test("Encodes and decodes SDP with IPv6 candidate")
        func encodeDecodeSdpWithIPv6Candidate() throws {
            let candidate = SdpCoderTests.makeIPv6Candidate()
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: [candidate]
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(decoded == input)
        }

        @Test("Encodes and decodes SDP with multiple candidates")
        func encodeDecodeSdpWithMultipleCandidates() throws {
            let candidates = [
                SdpCoderTests.makeIPv4Candidate(foundation: "1", ip: "192.168.1.1", port: 1_001),
                SdpCoderTests.makeIPv4Candidate(foundation: "2", ip: "192.168.1.2", port: 1_002),
                SdpCoderTests.makeIPv4Candidate(foundation: "3", ip: "10.0.0.1", port: 1_003, type: "srflx")
            ]
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: candidates
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(decoded.candidates.count == 3)
        }

        @Test("Preserves fingerprint through encoding cycle")
        func preservesFingerprintThroughEncodingCycle() throws {
            let input = SdpCoderSetup(
                setupSdp: SdpCoderTests.validOfferSdp,
                candidates: []
            )

            let encoded = try sut.encodeSetup(input)
            let decoded = try sut.decodeSetup(encoded)

            #expect(decoded.setupSdp
                .contains(
                    "sha-256 AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
                ))
        }

        @Test("Throws error for SDP missing ice-ufrag")
        func throwsForMissingIceUfrag() throws {
            let invalidSdp = """
            v=0
            o=- 1234567890 2 IN IP4 127.0.0.1
            s=-
            a=ice-pwd:testPassword123
            a=fingerprint:sha-256 AA:BB:CC:DD
            """
            let input = SdpCoderSetup(setupSdp: invalidSdp, candidates: [])

            #expect(throws: SdpCodingError.self) {
                try sut.encodeSetup(input)
            }
        }

        @Test("Throws error for SDP missing ice-pwd")
        func throwsForMissingIcePwd() throws {
            let invalidSdp = """
            v=0
            o=- 1234567890 2 IN IP4 127.0.0.1
            s=-
            a=ice-ufrag:testUfrag
            a=fingerprint:sha-256 AA:BB:CC:DD
            """
            let input = SdpCoderSetup(setupSdp: invalidSdp, candidates: [])

            #expect(throws: SdpCodingError.self) {
                try sut.encodeSetup(input)
            }
        }

        @Test("Throws error for SDP missing fingerprint")
        func throwsForMissingFingerprint() throws {
            let invalidSdp = """
            v=0
            o=- 1234567890 2 IN IP4 127.0.0.1
            s=-
            a=ice-ufrag:testUfrag
            a=ice-pwd:testPassword123
            """
            let input = SdpCoderSetup(setupSdp: invalidSdp, candidates: [])

            #expect(throws: SdpCodingError.self) {
                try sut.encodeSetup(input)
            }
        }
    }
}

// MARK: - Candidate Encoding/Decoding Tests

extension SdpCoderTests {
    struct CandidateEncodingTests {
        let sut = SdpCoder()

        @Test("Encodes and decodes empty candidates array")
        func encodeDecodeEmptyCandidates() throws {
            let encoded = try sut.encodeCandidates([])
            let decoded = try sut.decodeCandidates(encoded)

            #expect(decoded.isEmpty)
        }

        @Test("Encodes and decodes multiple mixed candidates")
        func encodesDecodesMultipleMixedCandidates() throws {
            let candidates = [
                SdpCoderTests.makeIPv4Candidate(foundation: "1", transport: "TCP", type: "host"),
                SdpCoderTests.makeIPv4Candidate(foundation: "2", type: "srflx"),
                SdpCoderTests.makeIPv6Candidate(foundation: "3", type: "relay"),
                SdpCoderTests.makeIPv6Candidate(foundation: "4", transport: "TCP", type: "prflx")
            ]

            let encoded = try sut.encodeCandidates(candidates)
            let decoded = try sut.decodeCandidates(encoded)

            #expect(decoded == candidates)
        }
    }
}

// MARK: - Error Handling Tests

extension SdpCoderTests {
    struct ErrorHandlingTests {
        let sut = SdpCoder()

        @Test("Throws error for invalid candidate format - missing prefix")
        func throwsForInvalidCandidateNoPrefix() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "invalid candidate string",
                sdpMLineIndex: 0,
                sdpMid: "0"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for empty foundation string")
        func throwsForEmptyFoundation() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate: 1 UDP 2130706431 192.168.1.1 54321 typ host",
                sdpMLineIndex: 0,
                sdpMid: "0"
            )

            #expect(
                throws: SdpCodingError.invalidCandidateFormat,
                performing: {
                    _ = try sut.encodeCandidates([candidate])
                }
            )
        }

        @Test("Throws error for candidate with unsupported sdpMLineIndex")
        func throwsForUnsupportedSdpMLineIndex() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 1 UDP 2130706431 192.168.1.1 54321 typ host",
                sdpMLineIndex: 1,
                sdpMid: "0"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for candidate with unsupported sdpMid")
        func throwsForUnsupportedSdpMid() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 1 UDP 2130706431 192.168.1.1 54321 typ host",
                sdpMLineIndex: 0,
                sdpMid: "1"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for candidate with unsupported component id")
        func throwsForUnsupportedComponentId() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 2 UDP 2130706431 192.168.1.1 54321 typ host",
                sdpMLineIndex: 0,
                sdpMid: "0"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for candidate with invalid IP address")
        func throwsForInvalidIpAddress() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 1 UDP 2130706431 invalid.ip.address 54321 typ host",
                sdpMLineIndex: 0,
                sdpMid: "0"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for candidate with too few parts")
        func throwsForCandidateWithTooFewParts() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 1 UDP 2130706431",
                sdpMLineIndex: 0,
                sdpMid: "0"
            )

            #expect(throws: SdpCodingError.self) {
                try sut.encodeCandidates([candidate])
            }
        }

        @Test("Throws error for invalid fingerprint format")
        func throwsForInvalidFingerprintFormat() throws {
            let invalidSdp = """
            v=0
            o=- 1234567890 2 IN IP4 127.0.0.1
            a=ice-ufrag:testUfrag
            a=ice-pwd:testPassword123
            a=fingerprint:sha-256 ZZZZ:INVALID
            """
            let input = SdpCoderSetup(setupSdp: invalidSdp, candidates: [])

            #expect(throws: SdpCodingError.self) {
                try sut.encodeSetup(input)
            }
        }

        @Test("Accepts candidate with nil sdpMid")
        func acceptsCandidateWithNilSdpMid() throws {
            let candidate = PeerConnectionCandidate(
                sdp: "candidate:1 1 UDP 2130706431 192.168.1.1 54321 typ host",
                sdpMLineIndex: 0,
                sdpMid: nil
            )

            let encoded = try sut.encodeCandidates([candidate])
            let decoded = try sut.decodeCandidates(encoded)

            #expect(decoded.count == 1)
        }
    }
}

// MARK: - IP Address Tests

extension SdpCoderTests {
    struct IPAddressTests {
        @Test("Parses valid IPv4 addresses")
        func parsesValidIPv4() {
            let testCases = [
                "0.0.0.0",
                "127.0.0.1",
                "192.168.1.1",
                "255.255.255.255",
                "10.0.0.1"
            ]

            for ip in testCases {
                let parsed = SdpCoder.IP4Address(fromString: ip)
                #expect(parsed != nil)
                #expect(parsed?.toString() == ip)
            }
        }

        @Test("Rejects invalid IPv4 addresses")
        func rejectsInvalidIPv4() {
            let testCases = [
                "256.0.0.1",
                "192.168.1",
                "192.168.1.1.1",
                "abc.def.ghi.jkl",
                "",
                "192.168.1.1.extra"
            ]

            for ip in testCases {
                #expect(SdpCoder.IP4Address(fromString: ip) == nil)
            }
        }

        @Test("IP4Address components are correct")
        func ip4ComponentsAreCorrect() {
            let ip = SdpCoder.IP4Address(fromString: "192.168.1.100")

            #expect(ip?.comp1 == 192)
            #expect(ip?.comp2 == 168)
            #expect(ip?.comp3 == 1)
            #expect(ip?.comp4 == 100)
        }

        @Test("Parses valid IPv6 addresses")
        func parsesValidIPv6() {
            let testCases = [
                "2001:db8:85a3:0:0:8a2e:370:7334",
                "fe80:0:0:0:0:0:0:1",
                "0:0:0:0:0:0:0:1"
            ]

            for ip in testCases {
                let parsed = SdpCoder.IP6Address(inputString: ip)
                #expect(parsed != nil)
            }
        }

        @Test("Parses IPv6 with compression")
        func parsesIPv6WithCompression() {
            let testCases = [
                "::1",
                "2001:db8::1",
                "fe80::1"
            ]

            for ip in testCases {
                let parsed = SdpCoder.IP6Address(inputString: ip)
                #expect(parsed != nil)
            }
        }
    }
}
