import Foundation
import WebRTC
import AsyncExtensions
import AsyncAlgorithms
import StructuredConcurrency

enum AsyncDataChannelWrapperError: Error {
    case closed
    case sendFailed
}

/// Wraps `RTCDataChannel` and serialises both *outbound operations*
/// (`send`, `close`) and *inbound delegate events* (state changes,
/// received buffers) through the shared `SerialOperationQueue` provided
/// by the parent `AsyncPeerConnectionWrapper`.
///
/// Delegate callbacks fire on WebRTC's data-channel thread, yield a
/// `DelegateEvent` into an internal `AsyncStream`, and a dedicated
/// consumer task drains that stream — each event applied via the same
/// `queue.run` callers use for sends. As a result, the state subject and
/// the received-message pipeline are never updated concurrently with an
/// outbound `send`.
///
/// Received buffers flow through a secondary producer task into the
/// public `messages` `AsyncChannel`, so back-pressure from a slow
/// consumer cannot block the serial queue.
///
/// Outbound sends are consumed by one owned FIFO drainer. Buffered-amount
/// callbacks only yield coalesced wakeups; every send decision reads the
/// native amount afresh and conditionally sends inside `queue.run`.
final class AsyncDataChannelWrapper: NSObject {
    private enum LifecycleState {
        case open
        case closing
        case closed
    }

    private enum DelegateEvent {
        case state(RTCDataChannelState)
        case message(RTCDataBuffer)
    }

    private static let sendWatermarkPolicy = DataChannelSendWatermarkPolicy(
        highWatermark: 4 * 1_024 * 1_024,
        lowWatermark: 1 * 1_024 * 1_024
    )

    let dataChannel: RTCDataChannel

    private let queue: SerialOperationQueue
    private lazy var sendDrainer = DataChannelSendDrainer<RTCDataBuffer>(
        attempt: { [weak self] buffer, byteCount, wasThrottled in
            guard let self else { return .closed }
            return await attemptSend(
                buffer,
                byteCount: byteCount,
                wasThrottled: wasThrottled
            )
        }
    )

    let state: AsyncCurrentValueSubject<RTCDataChannelState>
    let logger: LoggerProtocol
    let messages: AsyncChannel<RTCDataBuffer> = .init()

    private let delegateEvents: AsyncStream<DelegateEvent>
    private let delegateContinuation: AsyncStream<DelegateEvent>.Continuation
    private var delegateTask: Task<Void, Never>?

    private let messagesStream: AsyncStream<RTCDataBuffer>
    private let messagesContinuation: AsyncStream<RTCDataBuffer>.Continuation
    private var producerTask: Task<Void, Never>?

    private var lifecycle: LifecycleState = .open

    init(
        dataChannel: RTCDataChannel,
        queue: SerialOperationQueue,
        logger: LoggerProtocol
    ) {
        self.dataChannel = dataChannel
        self.queue = queue
        self.logger = logger
        state = .init(dataChannel.readyState)

        let (eventStream, eventContinuation) = AsyncStream<DelegateEvent>.makeStream(bufferingPolicy: .unbounded)
        delegateEvents = eventStream
        delegateContinuation = eventContinuation

        let (msgStream, msgContinuation) = AsyncStream<RTCDataBuffer>.makeStream(bufferingPolicy: .unbounded)
        messagesStream = msgStream
        messagesContinuation = msgContinuation

        super.init()

        // Initialize the lazy closure before `self` is published to WebRTC or callers.
        _ = sendDrainer
        self.dataChannel.delegate = self
        startMessageProducer()
        startDelegateConsumer()
    }

    deinit {
        delegateContinuation.finish()
        messagesContinuation.finish()
        messages.finish()
        sendDrainer.terminate(with: .closed)
        delegateTask?.cancel()
        producerTask?.cancel()

        logger.debug("Deinited")
    }

    // MARK: - Serialised WebRTC operations

    func send(_ buffer: RTCDataBuffer) async throws {
        try await sendDrainer.send(buffer, byteCount: UInt64(buffer.data.count))
    }

    func close() async {
        await queue.run { [self] in
            beginLocalClose()
        }

        sendDrainer.terminate(with: .closed)
        await sendDrainer.waitUntilFinished()

        await queue.run { [self] in
            finishLocalClose()
        }

        delegateContinuation.finish()
        messagesContinuation.finish()
        messages.finish()

        await delegateTask?.value
        await producerTask?.value
    }

    // MARK: - Private (queue-isolated)

    private func beginLocalClose() {
        if lifecycle == .open {
            lifecycle = .closing
            logger.debug("Closing data channel")
        }
        dataChannel.delegate = nil
    }

    private func finishLocalClose() {
        guard lifecycle != .closed else { return }

        dataChannel.close()
        transitionToClosed()
        logger.debug("Data channel closed")
    }

    private func transitionToClosed() {
        lifecycle = .closed
        state.send(.closed)
    }

    // MARK: - Delegate event pipeline

    private func startDelegateConsumer() {
        delegateTask = Task { [weak self, delegateEvents, queue] in
            for await event in delegateEvents {
                await queue.run { [weak self] in
                    self?.applyDelegateEvent(event)
                }
            }
        }
    }

    private func startMessageProducer() {
        producerTask = Task { [messages, messagesStream] in
            for await buffer in messagesStream {
                await messages.send(buffer)
            }
            messages.finish()
        }
    }

    /// Runs on the shared serial queue. Remote `.closing` prevents new
    /// sends but still permits already-delivered inbound tail messages;
    /// terminal `.closed` finishes inbound delivery and is published once.
    private func applyDelegateEvent(_ event: DelegateEvent) {
        switch event {
        case let .state(newState):
            logger.debug("State: \(newState)")
            applyDataChannelState(newState)
        case let .message(buffer):
            guard lifecycle != .closed else { return }
            logger.debug("Data channel: \(buffer.data.count)")
            messagesContinuation.yield(buffer)
        }
    }

    private func applyDataChannelState(_ newState: RTCDataChannelState) {
        switch newState {
        case .closing:
            guard lifecycle == .open else { return }
            lifecycle = .closing
            state.send(.closing)
            sendDrainer.terminate(with: .closed)
        case .closed:
            guard lifecycle != .closed else { return }
            transitionToClosed()
            sendDrainer.terminate(with: .closed)
            messagesContinuation.finish()
        case .connecting,
             .open:
            guard lifecycle == .open else { return }
            state.send(newState)
        @unknown default:
            guard lifecycle == .open else { return }
            state.send(newState)
        }
    }

    private func attemptSend(
        _ buffer: RTCDataBuffer,
        byteCount: UInt64,
        wasThrottled: Bool
    ) async -> DataChannelSendAttemptResult {
        await queue.run { [self] in
            guard lifecycle == .open else { return .closed }

            let bufferedAmount = dataChannel.bufferedAmount

            if Self.sendWatermarkPolicy.shouldWait(
                bufferedAmount: bufferedAmount,
                byteCount: byteCount,
                wasThrottled: wasThrottled
            ) {
                return .wait
            }

            return dataChannel.sendData(buffer) ? .sent : .failed
        }
    }
}

extension AsyncDataChannelWrapper: RTCDataChannelDelegate {
    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        delegateContinuation.yield(.state(dataChannel.readyState))
    }

    func dataChannel(_: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        delegateContinuation.yield(.message(buffer))
    }

    func dataChannel(_: RTCDataChannel, didChangeBufferedAmount _: UInt64) {
        sendDrainer.wakeUp()
    }
}
