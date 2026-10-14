import Foundation
import SubstrateSdk
import AsyncAlgorithms
import AsyncExtensions
import WebRTC

enum MultiplexedDataChannelError: Error {
    case sendFailed
}

final class MultiplexedDataChannel {
    private let dataChannelWrapper: AsyncDataChannelWrapper
    private let logger: LoggerProtocol

    private let lock = NSLock()
    private var subscribers: [String: AsyncChannel<Data>] = [:]
    private var isFinished = false
    private var demuxTask: Task<Void, Never>?

    init(dataChannelWrapper: AsyncDataChannelWrapper, logger: LoggerProtocol) {
        self.dataChannelWrapper = dataChannelWrapper
        self.logger = logger
    }

    deinit {
        demuxTask?.cancel()
        finishSubscribers()
        logger.debug("Deinited")
    }

    func start() {
        guard demuxTask == nil else { return }

        demuxTask = Task { [weak self, dataChannelWrapper] in
            defer { self?.finishSubscribers() }

            for await buffer in dataChannelWrapper.messages {
                guard !Task.isCancelled else { return }

                do {
                    let decoder = try ScaleDecoder(data: buffer.data)
                    let message = try DataChannelMessage(scaleDecoder: decoder)

                    let channel = self?.subscriber(for: message.id)

                    if let channel {
                        await channel.send(message.data)
                    } else {
                        self?.logger.warning("No subscriber for use case: \(message.id)")
                    }
                } catch {
                    self?.logger.error("DataChannelMessage decoding failed: \(error)")
                }
            }
        }
    }

    func subscribe(useCaseId: String) -> AnyAsyncSequence<Data> {
        lock.lock()
        defer { lock.unlock() }

        if let existing = subscribers[useCaseId] {
            return existing.eraseToAnyAsyncSequence()
        }

        let channel = AsyncChannel<Data>()

        guard !isFinished else {
            channel.finish()
            return channel.eraseToAnyAsyncSequence()
        }

        subscribers[useCaseId] = channel
        return channel.eraseToAnyAsyncSequence()
    }

    func send(data: Data, useCaseId: String) async throws {
        let message = DataChannelMessage(id: useCaseId, data: data)
        let encoded: Data
        do {
            encoded = try message.scaleEncoded()
        } catch {
            logger.error("DataChannelMessage encoding failed: \(error)")
            throw error
        }

        do {
            let buffer = RTCDataBuffer(data: encoded, isBinary: true)
            try await dataChannelWrapper.send(buffer)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            logger.error("DataChannel send failed: \(error)")
            throw MultiplexedDataChannelError.sendFailed
        }
    }
}

private extension MultiplexedDataChannel {
    func subscriber(for id: String) -> AsyncChannel<Data>? {
        lock.lock()
        defer { lock.unlock() }
        return subscribers[id]
    }

    func finishSubscribers() {
        lock.lock()
        isFinished = true
        let channels = subscribers.values
        subscribers.removeAll()
        lock.unlock()

        for channel in channels {
            channel.finish()
        }
    }
}
