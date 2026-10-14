import Foundation
import WebRTC
import SubstrateSdk
import AsyncExtensions

enum DeviceSyncDataChannelState {
    case connected
    case disconnected
}

enum DeviceSyncDataChannelError: Error {
    case connectionFailed
    case notConnected
}

protocol DeviceSyncPeerConnectionFlowing: Actor {
    var updates: AnyAsyncSequence<Chat.DeviceSyncUpdate> { get async }
    var acks: AnyAsyncSequence<Chat.DeviceSyncUpdateAck> { get async }
    var states: AnyAsyncSequence<DeviceSyncDataChannelState> { get async }

    func start() async throws
    func sendUpdate(_ update: Chat.DeviceSyncUpdate) async throws
    func sendAck(_ ack: Chat.DeviceSyncUpdateAck) async throws
    func cancel() async
}

actor DeviceSyncPeerConnectionFlow {
    private static let purpose = "sync"
    private static let useCaseId = "device-sync"

    private let signaler: DeviceSyncSignalingSession
    private let role: CallRole
    private let peerLogger: LoggerProtocol

    private let peerConnectionFactory: RTCPeerConnectionFactory
    private let configFactory: WebRTCConfigMaking

    private var multiplexedChannel: MultiplexedDataChannel?
    private var dataConnectionCreator: DataConnectionCreating?
    private var connectedState: PeerDataConnectionState.Connected?

    private let updateSubject = AsyncPassthroughSubject<Chat.DeviceSyncUpdate>()
    private let ackSubject = AsyncPassthroughSubject<Chat.DeviceSyncUpdateAck>()
    private let stateSubject = AsyncPassthroughSubject<DeviceSyncDataChannelState>()

    private var connectTask: Task<PeerDataConnectionState.Connected?, Never>?
    private var receiveTask: Task<Void, Never>?
    private var connectionMonitorTask: Task<Void, Never>?
    private var isClosed = false

    init(
        signaler: DeviceSyncSignalingSession,
        role: CallRole,
        configFactory: WebRTCConfigMaking,
        peerConnectionFactory: RTCPeerConnectionFactory,
        peerLogger: LoggerProtocol
    ) {
        self.signaler = signaler
        self.role = role
        self.configFactory = configFactory
        self.peerConnectionFactory = peerConnectionFactory
        self.peerLogger = peerLogger
    }

    deinit {
        peerLogger.debug("Deinit")
    }
}

// MARK: - Public API

extension DeviceSyncPeerConnectionFlow: DeviceSyncPeerConnectionFlowing {
    var updates: AnyAsyncSequence<Chat.DeviceSyncUpdate> {
        updateSubject.eraseToAnyAsyncSequence()
    }

    var acks: AnyAsyncSequence<Chat.DeviceSyncUpdateAck> {
        ackSubject.eraseToAnyAsyncSequence()
    }

    var states: AnyAsyncSequence<DeviceSyncDataChannelState> {
        stateSubject.eraseToAnyAsyncSequence()
    }

    func start() async throws {
        guard !isClosed else {
            throw CancellationError()
        }

        guard dataConnectionCreator == nil, multiplexedChannel == nil else {
            peerLogger.debug("Sync data channel connection already started")
            return
        }

        let creator = makeDataConnectionCreator()
        dataConnectionCreator = creator

        peerLogger.debug("Creating connection as \(role)...")

        let stateSequence: AnyAsyncSequence<PeerDataConnectionState>
        do {
            stateSequence = try await creator.connect()
        } catch {
            peerLogger.error("Creation failed: \(error)")
            await tearDownCreator()
            throw DeviceSyncDataChannelError.connectionFailed
        }

        peerLogger.debug("Waiting for connected state...")

        guard let connectedState = await waitForCancellableConnectedState(from: stateSequence) else {
            peerLogger.error("Connection failed")
            await tearDownCreator()
            throw DeviceSyncDataChannelError.connectionFailed
        }

        do {
            try Task.checkCancellation()
        } catch {
            await tearDownCreator()
            throw error
        }

        peerLogger.debug("Connection established")

        self.connectedState = connectedState
        await creator.throttle()
        dataConnectionCreator = nil

        guard !isClosed else {
            self.connectedState = nil
            await connectedState.dataChannel.close()
            await connectedState.connection.close()
            throw CancellationError()
        }

        let muxChannel = MultiplexedDataChannel(
            dataChannelWrapper: connectedState.dataChannel,
            logger: peerLogger
        )
        muxChannel.start()
        multiplexedChannel = muxChannel

        startReceiving(from: muxChannel)
        stateSubject.send(.connected)
        startConnectionMonitoring(connectedState)
    }

    func sendUpdate(_ update: Chat.DeviceSyncUpdate) async throws {
        let syncMessage = Chat.DeviceSyncMessage.update(update)
        try await sendSyncMessage(syncMessage)
    }

    func sendAck(_ ack: Chat.DeviceSyncUpdateAck) async throws {
        let syncMessage = Chat.DeviceSyncMessage.ack(ack)
        try await sendSyncMessage(syncMessage)
    }

    func cancel() async {
        isClosed = true
        cancelConnectTask()
        let receiveTask = receiveTask
        self.receiveTask = nil
        let connectionMonitorTask = connectionMonitorTask
        self.connectionMonitorTask = nil
        receiveTask?.cancel()
        connectionMonitorTask?.cancel()

        if let creator = dataConnectionCreator {
            await creator.cancel()
        }
        dataConnectionCreator = nil

        multiplexedChannel = nil

        let connectedState = connectedState
        self.connectedState = nil
        await connectedState?.dataChannel.close()
        await connectedState?.connection.close()

        await receiveTask?.value
        await connectionMonitorTask?.value
    }
}

// MARK: - Connection

private extension DeviceSyncPeerConnectionFlow {
    /// Releases the in-flight `dataConnectionCreator` when `connect()` fails
    /// partway through. Without this the RTCPeerConnection it built remains
    /// a zombie on the shared factory's signaling thread — exactly the leak
    /// that, when accumulated, blocks subsequent CallEngine setup.
    func tearDownCreator() async {
        if let creator = dataConnectionCreator {
            await creator.cancel()
        }
        dataConnectionCreator = nil
    }

    func makeDataConnectionCreator() -> DataConnectionCreating {
        switch role {
        case .initiator:
            DataConnectionInitiator(
                signaling: signaler,
                peerConnectionFactory: peerConnectionFactory,
                configFactory: configFactory,
                purpose: Self.purpose,
                logger: peerLogger
            )
        case .acceptor:
            DataConnectionAcceptor(
                signaling: signaler,
                peerConnectionFactory: peerConnectionFactory,
                configFactory: configFactory,
                logger: peerLogger
            )
        }
    }

    func waitForCancellableConnectedState(
        from stateSequence: AnyAsyncSequence<PeerDataConnectionState>
    ) async -> PeerDataConnectionState.Connected? {
        let task = Task { [weak self] in
            guard let self else { return nil as PeerDataConnectionState.Connected? }
            return await waitForConnected(from: stateSequence)
        }

        connectTask = task
        let connectedState = await withTaskCancellationHandler {
            await task.value
        } onCancel: { [weak self] in
            Task { await self?.cancelConnectTask() }
        }
        connectTask = nil

        return connectedState
    }

    func waitForConnected(
        from sequence: AnyAsyncSequence<PeerDataConnectionState>
    ) async -> PeerDataConnectionState.Connected? {
        do {
            for try await state in sequence {
                peerLogger.debug("State: \(state)")
                switch state {
                case let .connected(model):
                    return model
                case .disconnected:
                    return nil
                case .waiting,
                     .connecting:
                    continue
                }
            }
            return nil
        } catch {
            peerLogger.error("Connection state error: \(error)")
            return nil
        }
    }

    func cancelConnectTask() {
        connectTask?.cancel()
        connectTask = nil
    }
}

// MARK: - Sending / Receiving

private extension DeviceSyncPeerConnectionFlow {
    func startConnectionMonitoring(_ connectedState: PeerDataConnectionState.Connected) {
        connectionMonitorTask?.cancel()

        connectionMonitorTask = Task { [weak self] in
            await withTaskGroup(of: Void.self) { group in
                group.addTask {
                    await self?.observeDataChannelState(connectedState.dataChannel)
                }
                group.addTask {
                    await self?.observeIceConnectionState(connectedState.connection)
                }
            }
        }
    }

    func observeDataChannelState(_ wrapper: AsyncDataChannelWrapper) async {
        do {
            for try await state in wrapper.state.eraseToAnyAsyncSequence() {
                guard !Task.isCancelled else { return }

                if state == .closed {
                    peerLogger.warning("Sync data channel closed")
                    stateSubject.send(.disconnected)
                    return
                }
            }
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error("Sync data channel state observation failed: \(error)")
            stateSubject.send(.disconnected)
        }
    }

    func observeIceConnectionState(_ wrapper: AsyncPeerConnectionWrapper) async {
        do {
            for try await state in wrapper.iceConnectionState.eraseToAnyAsyncSequence() {
                guard !Task.isCancelled else { return }

                switch state {
                case .failed,
                     .closed:
                    peerLogger.warning("Sync ICE connection lost: \(String(describing: state))")
                    stateSubject.send(.disconnected)
                    return
                case .disconnected:
                    peerLogger.debug("Sync ICE temporarily disconnected; waiting for WebRTC recovery")
                case .new,
                     .checking,
                     .connected,
                     .completed,
                     .count,
                     nil:
                    continue
                @unknown default:
                    continue
                }
            }
        } catch {
            guard !Task.isCancelled else { return }
            peerLogger.error("Sync ICE state observation failed: \(error)")
            stateSubject.send(.disconnected)
        }
    }

    func startReceiving(from muxChannel: MultiplexedDataChannel) {
        let syncStream = muxChannel.subscribe(useCaseId: Self.useCaseId)

        receiveTask = Task { [weak self] in
            do {
                for try await data in syncStream {
                    guard !Task.isCancelled else { return }
                    await self?.handleReceivedData(data)
                }

                guard !Task.isCancelled else { return }
                await self?.handleReceiveTermination()
            } catch {
                guard !Task.isCancelled else { return }
                await self?.handleReceiveTermination(error)
            }
        }
    }

    func handleReceiveTermination(_ error: Error? = nil) {
        if let error {
            peerLogger.error("Sync data stream failed: \(error)")
        } else {
            peerLogger.warning("Sync data stream ended unexpectedly")
        }
        stateSubject.send(.disconnected)
    }

    func sendSyncMessage(_ syncMessage: Chat.DeviceSyncMessage) async throws {
        guard let muxChannel = multiplexedChannel else {
            peerLogger.warning("Cannot send sync data: channel not connected")
            throw DeviceSyncDataChannelError.notConnected
        }

        let syncData = try syncMessage.scaleEncoded()
        peerLogger.debug("Sending \(syncData.count) bytes")
        try await muxChannel.send(data: syncData, useCaseId: Self.useCaseId)
    }

    func handleReceivedData(_ data: Data) {
        peerLogger.debug("Received \(data.count) bytes")

        do {
            let syncDecoder = try ScaleDecoder(data: data)
            let syncMessage = try Chat.DeviceSyncMessage(scaleDecoder: syncDecoder)

            switch syncMessage {
            case let .update(update):
                updateSubject.send(update)
            case let .ack(ack):
                ackSubject.send(ack)
            }
        } catch {
            peerLogger.error("Sync message decoding failed: \(error)")
        }
    }
}
