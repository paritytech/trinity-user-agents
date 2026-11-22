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
