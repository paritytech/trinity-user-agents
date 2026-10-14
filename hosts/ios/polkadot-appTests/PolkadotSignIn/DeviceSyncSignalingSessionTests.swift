@testable import polkadot_app
import Foundation
import SubstrateSdk
import Testing

@Suite(.timeLimit(.minutes(1)))
struct DeviceSyncSignalingSessionTests {
    @Test("Reconnected signal send encodes reconnected content")
    func reconnectedSignalSendEncodesReconnectedContent() async throws {
        let transport = MockDeviceSyncMessageTransport()
        let signaler = DeviceSyncSignalingSession(
            transport: transport,
            role: .acceptor,
            peerLogger: MockLogger()
        )

        await signaler.sendReconnected(offerId: "offer-1")

        let sentMessage = try #require(transport.sentMessages.first)
        let decoder = try ScaleDecoder(data: sentMessage)
        let envelope = try Chat.DeviceSyncSignalingEnvelope(scaleDecoder: decoder)

        #expect(envelope.offerId == "offer-1")
        #expect(envelope.message == .reconnected)
    }

    @Test("Signal batch attaches candidates to offer setup")
    func signalBatchAttachesCandidatesToOfferSetup() async throws {
        let transport = MockDeviceSyncMessageTransport()
        let signaler = DeviceSyncSignalingSession(
            transport: transport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        let candidates = [
            SdpCoderTests.makeIPv4Candidate(foundation: "1", ip: "192.168.1.1", port: 1_001),
            SdpCoderTests.makeIPv4Candidate(foundation: "2", ip: "192.168.1.2", port: 1_002)
        ]

        try await signaler.send([
            .offer(SdpCoderTests.validOfferSdp),
            .candidates(candidates)
        ])

        let sentBatch = try #require(transport.sentMessageBatches.first)
        #expect(transport.sentMessageBatches.count == 1)
        #expect(sentBatch.count == 1)

        let envelopes = try sentBatch.map { data in
            let decoder = try ScaleDecoder(data: data)
            return try Chat.DeviceSyncSignalingEnvelope(scaleDecoder: decoder)
        }

        guard case let .offer(setupData) = envelopes[0].message else {
            Issue.record("Expected offer envelope")
            return
        }

        let setup = try SdpCoder().decodeSetup(setupData)
        #expect(setup.setupSdp == SdpCoderTests.validOfferSdp)
        #expect(setup.candidates == candidates)
    }

    @Test("Matching reconnected signal is selected for context handling")
    func matchingReconnectedSignalIsSelected() async throws {
        let transport = MockDeviceSyncMessageTransport()
        let signaler = DeviceSyncSignalingSession(
            transport: transport,
            role: .initiator,
            peerLogger: MockLogger()
        )

        try await signaler.send([.offer(SdpCoderTests.validOfferSdp)])

        let sentMessage = try #require(await transport.waitForSentMessageCount(1).first)
        let sentDecoder = try ScaleDecoder(data: sentMessage)
        let sentEnvelope = try Chat.DeviceSyncSignalingEnvelope(scaleDecoder: sentDecoder)

        let following = Chat.DeviceSyncSignalingEnvelope(
            offerId: sentEnvelope.offerId,
            message: .candidates(Data([1, 2, 3]))
        )
        let reconnected = await signaler.reconnection(
            in: [
                .init(offerId: sentEnvelope.offerId, message: .reconnected),
                following
            ]
        )
        #expect(reconnected?.offerId == sentEnvelope.offerId)
        #expect(reconnected?.following == [following])
    }

    @Test("Offer is rejected when its id cannot be persisted")
    func offerIsRejectedWhenPersistenceFails() async throws {
        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let encodedOffer = try #require(sourceTransport.sentMessages.first)
        let offer = try Chat.DeviceSyncSignalingEnvelope(
            scaleDecoder: ScaleDecoder(data: encodedOffer)
        )
        let acceptor = DeviceSyncSignalingSession(
            transport: MockDeviceSyncMessageTransport(),
            role: .acceptor,
            offerIdHandler: { _ in throw DeviceSyncOfferIdPersistenceError() },
            peerLogger: MockLogger()
        )

        await acceptor.handleIncomingEnvelopes([offer])

        #expect(await acceptor.currentOfferId() == nil)
    }

    @Test("Latest conflicting offer is retained for the replacement flow")
    func latestConflictingOfferIsRetained() async throws {
        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let offers = try sourceTransport.sentMessages.map {
            try Chat.DeviceSyncSignalingEnvelope(scaleDecoder: ScaleDecoder(data: $0))
        }
        let first = try #require(offers.first)
        let latest = try #require(offers.last)
        let acceptor = DeviceSyncSignalingSession(
            transport: MockDeviceSyncMessageTransport(),
            role: .acceptor,
            peerLogger: MockLogger()
        )

        await acceptor.handleIncomingEnvelopes([first])
        await acceptor.handleIncomingEnvelopes([latest])
        #expect(await acceptor.currentOfferId() == first.offerId)

        await acceptor.resetForReconnection()
        #expect(await acceptor.currentOfferId() == latest.offerId)
    }

    @Test("Reset preserves reconnect identity without retaining negotiation state")
    func resetSeparatesReconnectIdentityFromNegotiation() async throws {
        let sourceTransport = MockDeviceSyncMessageTransport()
        let initiator = DeviceSyncSignalingSession(
            transport: sourceTransport,
            role: .initiator,
            peerLogger: MockLogger()
        )
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        try await initiator.send([.offer(SdpCoderTests.validOfferSdp)])
        let offers = try sourceTransport.sentMessages.map {
            try Chat.DeviceSyncSignalingEnvelope(scaleDecoder: ScaleDecoder(data: $0))
        }
        let first = try #require(offers.first)
        let next = try #require(offers.last)
        let acceptor = DeviceSyncSignalingSession(
            transport: MockDeviceSyncMessageTransport(),
            role: .acceptor,
            peerLogger: MockLogger()
        )

        await acceptor.handleIncomingEnvelopes([first])
        await acceptor.resetForReconnection()

        #expect(await acceptor.currentOfferId() == nil)
        #expect(await acceptor.reconnection(in: [
            .init(offerId: first.offerId, message: .reconnected)
        ])?.offerId == first.offerId)

        await acceptor.handleIncomingEnvelopes([next])
        #expect(await acceptor.currentOfferId() == next.offerId)
    }
}

private struct DeviceSyncOfferIdPersistenceError: Error {}
