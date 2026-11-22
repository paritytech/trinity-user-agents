@testable import polkadot_app
import Foundation
import Testing
import WebRTC

@Suite("Native Media optional ICE peer construction")
struct NativeMediaPeerConnectionTests {
    private enum Failure: Error, Equatable { case optional, direct }

    private func configuration(withServers: Bool = true) -> RTCConfiguration {
        let configuration = RTCConfiguration()
        configuration.sdpSemantics = .unifiedPlan
        configuration.iceTransportPolicy = .all
        configuration.iceCandidatePoolSize = 8
        if withServers {
            configuration.iceServers = [RTCIceServer(urlStrings: ["turn:relay.example:3478"],
                username: "test", credential: "test")]
        }
        return configuration
    }

    @Test("Rejected optional servers retry empty ICE without changing policy", arguments: [false, true])
    func rejectedConfigurationRetries(throwing: Bool) throws {
        var attempts = 0
        let connection = try makeNativeMediaPeerConnection(configuration: configuration()) { configuration in
            attempts += 1
            #expect(configuration.iceTransportPolicy == .all)
            #expect(configuration.sdpSemantics == .unifiedPlan)
            #expect(configuration.iceCandidatePoolSize == 8)
            if attempts == 1 {
                #expect(!configuration.iceServers.isEmpty)
                if throwing { throw Failure.optional }
            } else {
                #expect(configuration.iceServers.isEmpty)
            }
            return nil
        }
        #expect(connection == nil)
        #expect(attempts == 2)
    }

    @Test("Successful direct retry returns the native peer")
    func directRetrySucceeds() throws {
        let factory = RTCPeerConnectionFactory()
        var attempts = 0
        let connection = try makeNativeMediaPeerConnection(configuration: configuration()) { configuration in
            attempts += 1
            if attempts == 1 { return nil }
            return factory.peerConnection(with: configuration,
                constraints: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil), delegate: nil)
        }
        defer { connection?.close() }
        #expect(connection != nil)
        #expect(attempts == 2)
    }

    @Test("Direct factory errors survive without another retry", arguments: [false, true])
    func directErrorIsPreserved(configured: Bool) {
        var attempts = 0
        #expect(throws: Failure.direct) {
            try makeNativeMediaPeerConnection(configuration: configuration(withServers: configured)) { _ in
                attempts += 1
                if configured && attempts == 1 { return nil }
                throw Failure.direct
            }
        }
        #expect(attempts == (configured ? 2 : 1))
    }

    @Test("A nil direct peer is not retried")
    func directNilIsPreserved() throws {
        var attempts = 0
        let connection = try makeNativeMediaPeerConnection(configuration: configuration(withServers: false)) { _ in
            attempts += 1
            return nil
        }
        #expect(connection == nil)
        #expect(attempts == 1)
    }

    @Test("Outer cancellation prevents peer construction")
    func cancelledBeforeConstruction() async {
        let task = Task<Void, Error> {
            withUnsafeCurrentTask { $0?.cancel() }
            _ = try makeNativeMediaPeerConnection(configuration: configuration()) { _ in
                Issue.record("A cancelled operation must not construct a peer")
                return nil
            }
        }
        await #expect(throws: CancellationError.self) { try await task.value }
    }

    @Test("Outer cancellation during optional failure prevents direct retry", arguments: [false, true])
    func cancelledDuringConstruction(throwing: Bool) async {
        let task = Task<Void, Error> {
            var attempts = 0
            defer { #expect(attempts == 1) }
            _ = try makeNativeMediaPeerConnection(configuration: configuration()) { _ in
                attempts += 1
                withUnsafeCurrentTask { $0?.cancel() }
                if throwing { throw Failure.optional }
                return nil
            }
        }
        await #expect(throws: CancellationError.self) { try await task.value }
    }
}

@Suite("Native Media half-trickle")
struct NativeMediaHalfTrickleTests {
    @Test("Gathered candidates are embedded, including delayed callback delivery")
    func gatheredCandidatesAreNotTrickled() {
        var ice = NativeMediaLocalIce()
        ice.begin()
        #expect(!ice.candidate(candidate()))
        #expect(ice.publish(sdp()).isEmpty)
        // Callback serialization carries extensions absent from the SDP.
        #expect(!ice.candidate(candidate()))
        #expect(ice.candidate(candidate(port: 5001)))
        #expect(!ice.candidate(candidate(port: 5001)))
    }

    @Test("A candidate arriving after the SDP snapshot is retained in description-first order")
    func snapshotBoundaryPreservesLateCandidates() {
        var ice = NativeMediaLocalIce()
        #expect(!ice.candidate(candidate()))
        #expect(!ice.candidate(candidate(port: 5001)))
        #expect(!ice.candidate(candidate(mline: 1)))
        let late = ice.publish(sdp())
        #expect(late.map(\.sdpMLineIndex) == [0, 1])
        #expect(late[0].sdp.contains(" 5001 typ host"))
        #expect(late[1].sdp.contains(" 5000 typ host"))
        #expect(!ice.candidate(candidate(port: 5001)))
    }

    @Test("The deadline may publish an empty candidate snapshot and later ICE still trickles")
    func partialGatheringStillTrickles() {
        var ice = NativeMediaLocalIce()
        #expect(ice.publish(sdp(includeCandidate: false)).isEmpty)
        #expect(ice.candidate(candidate()))
        #expect(!ice.candidate(candidate()))
    }

    @Test("Answers and later offers reopen the gate; restarted ICE rejects stale credentials")
    func subsequentDescriptionsUseTheirOwnGeneration() {
        var ice = NativeMediaLocalIce()
        #expect(ice.publish(sdp()).isEmpty)
        ice.begin()
        #expect(!ice.candidate(candidate(port: 5002)))
        #expect(!ice.candidate(candidate(ufrag: "restart")))
        #expect(ice.publish(sdp(ufrag: "restart")).isEmpty)
        #expect(!ice.candidate(candidate(port: 5003)))
        #expect(!ice.candidate(candidate(ufrag: "restart")))
        #expect(ice.candidate(candidate(ufrag: "restart", port: 5004)))
    }

    @Test("Per-media ICE credentials override session credentials")
    func mediaCredentialsOverrideSessionCredentials() {
        var ice = NativeMediaLocalIce()
        let description = sdp() + "a=ice-ufrag:video\r\n"
        #expect(ice.publish(description).isEmpty)
        #expect(!ice.candidate(candidate(mline: 1)))
        #expect(ice.candidate(candidate(ufrag: "video", mline: 1)))
    }

    @Test("Cancellation discards buffered and future callbacks until a new negotiation")
    func cancellationDiscardsCandidates() {
        var ice = NativeMediaLocalIce()
        #expect(!ice.candidate(candidate()))
        ice.cancel()
        #expect(!ice.candidate(candidate(port: 5001)))
        ice.begin()
        #expect(ice.publish(sdp(includeCandidate: false)).isEmpty)
        #expect(ice.candidate(candidate(port: 5002)))
    }

    @Test("Pending candidate capacity drops excess even when buffered duplicates are embedded")
    func pendingCandidateCapacity() {
        var ice = NativeMediaLocalIce()
        for _ in 0..<nativeMediaLocalCandidateLimit {
            #expect(!ice.candidate(candidate()))
        }
        #expect(!ice.candidate(candidate(port: 5001)))
        #expect(ice.publish(sdp()).isEmpty)
        // Overflow was not retained; callbacks after publication may still use
        // the remaining advertised budget.
        #expect(ice.candidate(candidate(port: 5002)))
        ice.begin()
        #expect(!ice.candidate(candidate(ufrag: "restart", port: 5003)))
        #expect(ice.publish(sdp(ufrag: "restart")).count == 1)
    }

    @Test("Embedded and continually gathered candidates share a bounded generation budget")
    func advertisedCandidateCapacityResets() {
        var ice = NativeMediaLocalIce()
        #expect(ice.publish(sdp()).isEmpty)
        for index in 0..<(nativeMediaLocalCandidateLimit - 1) {
            #expect(ice.candidate(candidate(port: 5001 + index)))
        }
        #expect(!ice.candidate(candidate(port: 5000 + nativeMediaLocalCandidateLimit)))
        #expect(!ice.candidate(candidate()))
        ice.begin()
        #expect(ice.publish(sdp(ufrag: "restart")).isEmpty)
        #expect(ice.candidate(candidate(ufrag: "restart", port: 6000)))
        #expect(!ice.candidate(candidate(ufrag: "first", port: 6001)))
    }

    private func candidate(ufrag: String = "first", port: Int = 5000, mline: Int32 = 0) -> RTCIceCandidate {
        RTCIceCandidate(
            sdp: "candidate:1 1 udp 1234 192.0.2.1 \(port) typ host generation 0 ufrag \(ufrag) network-cost 10",
            sdpMLineIndex: mline, sdpMid: String(mline))
    }

    private func sdp(ufrag: String = "first", includeCandidate: Bool = true) -> String {
        "v=0\r\na=ice-ufrag:\(ufrag)\r\n" +
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:0\r\n" +
            (includeCandidate ? "a=candidate:1 1 udp 1234 192.0.2.1 5000 typ host generation 0\r\n" : "") +
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:1\r\n"
    }
}
