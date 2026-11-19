import Foundation
import SubstrateSdk
import AsyncExtensions

final class CallMediaStateChannel {
    static let useCaseId = "webrtc_media_state_use_case"

    let signals: AnyAsyncSequence<CallMediaStateSignal>

    private let multiplexedChannel: MultiplexedDataChannel

    init(multiplexedChannel: MultiplexedDataChannel, logger: LoggerProtocol) {
        self.multiplexedChannel = multiplexedChannel

        signals = multiplexedChannel
            .subscribe(useCaseId: Self.useCaseId)
            .compactMap { data in
                do {
                    let decoder = try ScaleDecoder(data: data)
                    return try CallMediaStateSignal(scaleDecoder: decoder)
                } catch {
                    // The multiplexer demuxes on one task and back-pressures on send, so a
                    // subscriber that stops draining stalls every use case on the connection,
                    // renegotiation included. Drop the frame instead of propagating.
                    logger.error("Media state decoding failed: \(error)")
                    return nil
                }
            }
            .eraseToAnyAsyncSequence()
    }

    func send(_ signal: CallMediaStateSignal) async throws {
        let data = try signal.scaleEncoded()
        try await multiplexedChannel.send(data: data, useCaseId: Self.useCaseId)
    }
}
