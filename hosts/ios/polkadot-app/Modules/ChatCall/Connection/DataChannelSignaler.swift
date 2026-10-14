import Foundation
import SubstrateSdk
import AsyncExtensions
import WebRTC

final class DataChannelSignaler {
    static let useCaseId = "webrtc_renegotiation_internal_use_case"

    let multiplexedChannel: MultiplexedDataChannel
    let logger: LoggerProtocol

    private let subscribedStream: AnyAsyncSequence<Data>

    init(multiplexedChannel: MultiplexedDataChannel, logger: LoggerProtocol) {
        self.multiplexedChannel = multiplexedChannel
        self.logger = logger

        // Subscribe immediately so the multiplexed demux task has a channel
        // to deliver messages to, even before anyone iterates `signals`.
        subscribedStream = multiplexedChannel.subscribe(useCaseId: Self.useCaseId)
    }
}

extension DataChannelSignaler: PeerConnectionSignaling {
    var signals: AnyAsyncSequence<PeerConnectionSignal> {
        subscribedStream
            .compactMap { [logger] data in
                do {
                    logger.debug("Processing buffer: \(data.count)")

                    let decoder = try ScaleDecoder(data: data)
                    return try PeerConnectionSignal(scaleDecoder: decoder)
                } catch {
                    logger.error("Buffer decoding failed: \(error)")
                    return nil
                }
            }
            .eraseToAnyAsyncSequence()
    }

    @discardableResult
    func send(
        _ signals: [PeerConnectionSignal]
    ) async throws -> PeerConnectionSignalSendResult {
        var isFullySent = true

        for signal in signals {
            do {
                let data = try signal.scaleEncoded()
                try await multiplexedChannel.send(data: data, useCaseId: Self.useCaseId)
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                logger.error("Signal send failed: \(error)")
                isFullySent = false
            }
        }

        return PeerConnectionSignalSendResult(isFullySent: isFullySent)
    }
}
