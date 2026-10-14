import Foundation
import SubstrateSdk
import BigInt

struct SdpCoderSetup: Equatable {
    let setupSdp: String
    let candidates: [PeerConnectionCandidate]
}

enum SdpCodingError: Error, Equatable {
    case invalidSdpFormat
    case missingRequiredField(String)
    case decodingFailed
    case invalidCandidateFormat
    case invalidFingerprintFormat
    case invalidSessionLine
    case unsupportedSdpLineIndex(UInt32)
    case unsupportedSdpMid(String?)
    case unsupportedComponentId(UInt8)
}

protocol SdpCoding {
    func encodeSetup(_ input: SdpCoderSetup) throws -> Data
    func decodeSetup(_ data: Data) throws -> SdpCoderSetup
    func encodeCandidates(_ candidates: [PeerConnectionCandidate]) throws -> Data
    func decodeCandidates(_ data: Data) throws -> [PeerConnectionCandidate]
}

/// Minimum viable SDP base data
/// Based on minimal-webrtc: https://github.com/fippo/minimal-webrtc
final class SdpCoder {
    static let expectedComponentId: Int = 1

    init() {}

    // swiftlint:disable cyclomatic_complexity
    /// Parses an SDP candidate string into a strongly-typed `MinimalCandidate`.
    ///
    /// Exposed as `static` so callers outside the encoding pipeline (e.g. local-candidate
    /// filtering before sending) can reuse the same parser instead of re-implementing
    /// the RFC 5245 token split. Throws for malformed candidates or unsupported variants
    /// (multi-component, non-data-channel m-line, unknown transport/type).
    static func parseCandidate(_ candidate: PeerConnectionCandidate) throws -> MinimalCandidate {
        guard candidate.sdpMLineIndex == 0 else {
            throw SdpCodingError.unsupportedSdpLineIndex(candidate.sdpMLineIndex)
        }

        guard candidate.sdpMid == nil || candidate.sdpMid == "0" else {
            throw SdpCodingError.unsupportedSdpMid(candidate.sdpMid)
        }

        let trimmed = candidate.sdp.trimmingCharacters(in: .whitespaces)

        guard trimmed.hasPrefix("candidate:") else {
            throw SdpCodingError.invalidCandidateFormat
        }

        let content = String(trimmed.dropFirst("candidate:".count))
        let parts = content.split(separator: " ", omittingEmptySubsequences: true)

        guard parts.count >= 8 else {
            throw SdpCodingError.invalidCandidateFormat
        }

        let foundation = String(parts[0])

        guard let componentId = UInt8(parts[1]),
              let priority = UInt32(parts[3]) else {
            throw SdpCodingError.invalidCandidateFormat
        }

        guard componentId == Self.expectedComponentId else {
            throw SdpCodingError.unsupportedComponentId(componentId)
        }

        let transportString = String(parts[2])

        guard
            let transportType = MinimalCandidate.TransportType(
                transportString: transportString
            ) else {
            throw SdpCodingError.invalidCandidateFormat
        }

        let addressString = String(parts[4])
        guard let address = MinimalCandidate.IPAddress(addressString: addressString) else {
            throw SdpCodingError.invalidCandidateFormat
        }

        guard let port = UInt16(parts[5]) else {
            throw SdpCodingError.invalidCandidateFormat
        }

        guard parts[6] == "typ" else {
            throw SdpCodingError.invalidCandidateFormat
        }

        let typString = String(parts[7]).lowercased()
        guard let candidateType = MinimalCandidate.CandidateType(typeString: typString) else {
            throw SdpCodingError.invalidCandidateFormat
        }

        return MinimalCandidate(
            foundation: foundation,
            priority: priority,
            transportType: transportType,
            address: address,
            port: port,
            candidateType: candidateType
        )
    }
    // swiftlint:enable cyclomatic_complexity
}

extension PeerConnectionCandidate {
    /// `true` when this candidate uses TCP transport with `typ host`.
    ///
    /// TCP host candidates almost never succeed in P2P connections (they require both
    /// endpoints to accept inbound TCP connections), yet they inflate the signaling
    /// volume substantially — observed at ~44% of generated candidates in some logs.
    /// Filtering them out at the sender saves StatementStore submissions during
    /// pre-connection bursts. TCP relay candidates (TURN over TCP) are kept since
    /// they remain a useful fallback for highly restrictive networks.
    ///
    /// Backed by `SdpCoder.parseCandidate` so this filter and the encoding pipeline
    /// share one RFC 5245 tokenizer (with shared test coverage). Returns `false` for
    /// malformed candidates — they will fail downstream encoding anyway.
    var isTCPHost: Bool {
        guard let parsed = try? SdpCoder.parseCandidate(self) else { return false }
        return parsed.transportType == .tcp && parsed.candidateType == .host
    }
}

extension SdpCoder: SdpCoding {
    func encodeSetup(_ input: SdpCoderSetup) throws -> Data {
        // Extract minimal base from SDP
        let minimalBase = try extractMinimalBase(from: input.setupSdp)

        // Extract sessionId and sessionVersion from "o=" line
        let (sessionId, sessionVersion) = try extractSessionInfo(from: input.setupSdp)

        // Parse fingerprint string to Data
        let fingerprintData = try parseFingerprint(minimalBase.fingerprint)

        // Determine SDP type
        let sdpType: SdpCoder.SdpType = minimalBase.sdpType == "offer" ? .offer : .answer

        // Parse candidates
        let minimalCandidates = try input.candidates.map { try Self.parseCandidate($0) }

        // Create MinimalSetup
        let minimalSetup = SdpCoder.MinimalSetup(
            sdpType: sdpType,
            sessionId: sessionId,
            sessionVersion: sessionVersion,
            iceUfrag: minimalBase.iceUfrag,
            icePwd: minimalBase.icePwd,
            fingerprint: fingerprintData,
            candidates: minimalCandidates
        )

        // Scale encode
        let encoder = ScaleEncoder()
        try minimalSetup.encode(scaleEncoder: encoder)
        return encoder.encode()
    }

    func decodeSetup(_ data: Data) throws -> SdpCoderSetup {
        // Scale decode
        let decoder = try ScaleDecoder(data: data)
        let minimalSetup = try SdpCoder.MinimalSetup(scaleDecoder: decoder)

        // Reconstruct SDP base
        let sdpBase = try reconstructSdpBase(from: minimalSetup)

        // Reconstruct candidates
        let candidates = try minimalSetup.candidates.map { try reconstructCandidate($0) }

        return SdpCoderSetup(setupSdp: sdpBase, candidates: candidates)
    }

    func encodeCandidates(_ candidates: [PeerConnectionCandidate]) throws -> Data {
        let minimalCandidates = try candidates.map { try Self.parseCandidate($0) }

        let encoder = ScaleEncoder()
        try minimalCandidates.encode(scaleEncoder: encoder)
        return encoder.encode()
    }

    func decodeCandidates(_ data: Data) throws -> [PeerConnectionCandidate] {
        let decoder = try ScaleDecoder(data: data)
        let minimalCandidates = try [SdpCoder.MinimalCandidate](scaleDecoder: decoder)

        return try minimalCandidates.map {
            try reconstructCandidate($0)
        }
    }
}

// MARK: - Private Implementation

private extension SdpCoder {
    // MARK: - Base SDP Extraction

    struct MinimalSdpBase {
        let iceUfrag: String
        let icePwd: String
        let fingerprint: String
        let sdpType: String
    }

    /// Extracts only the essential base fields from a full SDP (without candidates)
    func extractMinimalBase(from sdp: String) throws -> MinimalSdpBase {
        let lines = sdp.components(separatedBy: .newlines)

        var iceUfrag: String?
        var icePwd: String?
        var fingerprint: String?
        var sdpType = "offer" // Default to offer

        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            // Extract ICE username fragment
            if trimmed.hasPrefix("a=ice-ufrag:") {
                iceUfrag = String(trimmed.dropFirst("a=ice-ufrag:".count))
            }

            // Extract ICE password
            if trimmed.hasPrefix("a=ice-pwd:") {
                icePwd = String(trimmed.dropFirst("a=ice-pwd:".count))
            }

            // Extract DTLS fingerprint
            if trimmed.hasPrefix("a=fingerprint:") {
                fingerprint = String(trimmed.dropFirst("a=fingerprint:".count))
            }

            // Extract SDP type (offer/answer)
            if trimmed.hasPrefix("a=setup:") {
                let setup = String(trimmed.dropFirst("a=setup:".count))
                // setup:actpass typically means offer, setup:active/passive means answer
                if setup == "actpass" {
                    sdpType = "offer"
                } else {
                    sdpType = "answer"
                }
            }
        }

        guard let iceUfrag,
              let icePwd,
              let fingerprint else {
            let missingFields = [
                iceUfrag == nil ? "ice-ufrag" : nil,
                icePwd == nil ? "ice-pwd" : nil,
                fingerprint == nil ? "fingerprint" : nil
            ].compactMap { $0 }

            throw SdpCodingError.missingRequiredField(missingFields.joined(separator: ", "))
        }

        return MinimalSdpBase(
            iceUfrag: iceUfrag,
            icePwd: icePwd,
            fingerprint: fingerprint,
            sdpType: sdpType
        )
    }

    /// Extracts sessionId and sessionVersion from "o=" line
    /// Format: "o=- sessionId sessionVersion IN IP4 address"
    func extractSessionInfo(from sdp: String) throws -> (BigUInt, BigUInt) {
        let lines = sdp.components(separatedBy: .newlines)

        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("o=") {
                let content = String(trimmed.dropFirst(2))
                let parts = content.split(separator: " ", omittingEmptySubsequences: true)

                guard parts.count >= 3 else {
                    throw SdpCodingError.invalidSessionLine
                }

                // Skip username (usually "-"), parse sessionId and sessionVersion
                guard let sessionId = BigUInt(String(parts[1])),
                      let sessionVersion = BigUInt(String(parts[2])) else {
                    throw SdpCodingError.invalidSessionLine
                }

                return (sessionId, sessionVersion)
            }
        }

        // Default values if not found
        let timestamp = BigUInt(Date().timeIntervalSince1970)
        return (timestamp, timestamp)
    }

    // MARK: - Candidate Reconstruction (Private)

    /// Reconstructs a candidate SDP string from MinimalCandidate
    func reconstructCandidate(_ candidate: SdpCoder.MinimalCandidate) throws -> PeerConnectionCandidate {
        let params = [
            candidate.foundation,
            String(Self.expectedComponentId),
            candidate.transportType.stringValue,
            String(candidate.priority),
            candidate.address.stringValue,
            String(candidate.port),
            "typ",
            candidate.candidateType.stringValue
        ].joined(with: .space)

        let sdp = "candidate:\(params)"

        return PeerConnectionCandidate(sdp: sdp, sdpMLineIndex: 0, sdpMid: "0")
    }

    // MARK: - SDP Reconstruction

    /// Reconstructs SDP base string from MinimalSetup components
    func reconstructSdpBase(
        from model: SdpCoder.MinimalSetup
    ) throws -> String {
        let setup = model.sdpType.stringValue

        // Use a placeholder IP - actual candidates will provide the real IP
        let placeholderIp = "0.0.0.0"

        // Reconstruct fingerprint string
        let fingerprintString = formatFingerprint(model.fingerprint)

        return """
        v=0
        o=- \(model.sessionId) \(model.sessionVersion) IN IP4 \(placeholderIp)
        s=-
        t=0 0
        m=application 9 UDP/DTLS/SCTP webrtc-datachannel
        c=IN IP4 \(placeholderIp)
        a=ice-ufrag:\(model.iceUfrag)
        a=ice-pwd:\(model.icePwd)
        a=fingerprint:\(fingerprintString)
        a=setup:\(setup)
        a=mid:0
        a=sctp-port:5000

        """
    }

    /// Parses fingerprint string to Data
    /// Format: "sha-256 <hex>" or just "<hex>"
    func parseFingerprint(_ fingerprint: String) throws -> Data {
        let trimmed = fingerprint.trimmingCharacters(in: .whitespaces)

        // Remove algorithm prefix if present (e.g., "sha-256 ")
        let hexString: String =
            if let spaceIndex = trimmed.firstIndex(of: " ") {
                String(trimmed[trimmed.index(after: spaceIndex)...])
            } else {
                trimmed
            }

        // Remove colons if present (e.g., "AA:BB:CC" -> "AABBCC")
        let cleanHex = hexString.replacingOccurrences(of: ":", with: "")

        guard let data = try? Data(hexString: cleanHex) else {
            throw SdpCodingError.invalidFingerprintFormat
        }

        return data
    }

    /// Formats fingerprint Data back to string
    func formatFingerprint(_ data: Data) -> String {
        let hexString = data.map { String(format: "%02X", $0) }.joined(separator: ":")
        return "sha-256 \(hexString)"
    }
}
