import Foundation
import WebRTC
import AsyncExtensions
import StructuredConcurrency

enum CallEngineError: Error {
    case peerConnectionClosed
    case endOfStream
    case frontCameraUnavailable
    case videoCaptureParamsUnavailable
}

protocol CallEngineProtocol: AnyObject {
    func observeState() -> AnyAsyncSequence<CallEngineState>
    func observeRemoteMediaState() -> AnyAsyncSequence<CallRemoteMediaState>
    func connect()
    func endCall(notifiesRemote: Bool) async

    func attach(localRenderer: RTCVideoRenderer)
    func attach(remoteRenderer: RTCVideoRenderer)

    func observeVideoState() -> AnyAsyncSequence<Bool>
    func observeVideoCaptureFailure() -> AnyAsyncSequence<Void>

    var isMuted: Bool { get async }
    func setMuted(_ isMuted: Bool) async -> Bool

    var isVideoEnabled: Bool { get async }
    func setVideoEnabled(_ isEnabled: Bool) async
}

private extension RTCVideoCapturer {
    func stopAnyCapture() {
        switch self {
        case let cameraCapturer as RTCCameraVideoCapturer:
            cameraCapturer.stopCapture()
        case let fileCapturer as RTCFileVideoCapturer:
            fileCapturer.stopCapture()
        default:
            break
        }
    }
}

private actor CallEngineActor {
    var videoCapturer: RTCVideoCapturer?

    var localTracks: CallTracks?
    var localRenderer: RTCVideoRenderer?

    var remoteTracks: CallTracks?
    var remoteRenderer: RTCVideoRenderer?

    var callCreator: CallCreatorProtocol?
    var connectionWrapper: AsyncPeerConnectionWrapper?
    var mediaStateChannel: CallMediaStateChannel?

    var isMuted: Bool = false
    var isVideoEnabled: Bool
    var hasEnded: Bool = false

    init(isVideoEnabled: Bool) {
        self.isVideoEnabled = isVideoEnabled
    }

    func markEndedIfFirst() -> Bool {
        guard !hasEnded else { return false }
        hasEnded = true
        return true
    }

    func setLocalTracks(_ tracks: CallTracks) {
        localTracks = tracks

        if let audioTrack = localTracks?.audioTrack {
            audioTrack.isEnabled = !isMuted
        }

        applyVideoEnabledState()
    }

    func getLocalTracks() -> CallTracks? {
        localTracks
    }

    func makeVideoCapturerIfNeeded(for track: RTCVideoTrack) -> RTCVideoCapturer? {
        guard isVideoEnabled, videoCapturer == nil, localRenderer != nil else {
            return nil
        }

        #if targetEnvironment(simulator)
            let capturer = RTCFileVideoCapturer(delegate: track.source)
        #else
            let capturer = RTCCameraVideoCapturer(delegate: track.source)
        #endif

        videoCapturer = capturer
        return capturer
    }

    func setLocalRenderer(_ renderer: RTCVideoRenderer) {
        localRenderer = renderer
    }

    func setRemoteTracks(_ tracks: CallTracks?) -> CallEngine.RemoteVideoTrackChange {
        let oldVideoTrack = remoteTracks?.videoTrack
        remoteTracks = tracks

        return CallEngine.RemoteVideoTrackChange(
            oldVideoTrack: oldVideoTrack,
            newVideoTrack: tracks?.videoTrack,
            renderer: remoteRenderer
        )
    }

    func setRemoteRenderer(_ renderer: RTCVideoRenderer) -> CallEngine.RemoteRendererBinding {
        let oldRenderer = remoteRenderer
        remoteRenderer = renderer

        return CallEngine.RemoteRendererBinding(
            oldRenderer: oldRenderer,
            videoTrack: remoteTracks?.videoTrack
        )
    }

    func setCallCreator(_ callCreator: CallCreatorProtocol) {
        self.callCreator = callCreator
    }

    func setConnectionWrapper(_ connectionWrapper: AsyncPeerConnectionWrapper) {
        self.connectionWrapper = connectionWrapper
    }

    func setMediaStateChannel(_ channel: CallMediaStateChannel) {
        mediaStateChannel = channel
    }

    func clearVideoCapture() {
        videoCapturer?.stopAnyCapture()
        videoCapturer = nil
    }

    func clearVideoCapture(_ capturer: RTCVideoCapturer) {
        guard videoCapturer === capturer else {
            return
        }

        clearVideoCapture()
    }

    func clearTracks() {
        localTracks?.audioTrack?.isEnabled = false
        localTracks?.videoTrack?.isEnabled = false

        localTracks = nil
        localRenderer = nil

        remoteTracks?.audioTrack?.isEnabled = false
        remoteTracks?.videoTrack?.isEnabled = false

        remoteTracks = nil
        remoteRenderer = nil

        mediaStateChannel = nil
        videoCapturer = nil
    }

    func clearCallCreator() {
        callCreator?.throttle()
        callCreator = nil
    }

    func toggleMute() -> Bool {
        setMuted(!isMuted)
    }

    func setMuted(_ isMuted: Bool) -> Bool {
        self.isMuted = isMuted

        guard let audioTrack = localTracks?.audioTrack else {
            return isMuted
        }

        audioTrack.isEnabled = !isMuted
        return isMuted
    }

    func getMuteState() -> Bool {
        isMuted
    }

    func setVideoEnabled(_ isEnabled: Bool) {
        isVideoEnabled = isEnabled
    }

    func applyVideoEnabledState() {
        localTracks?.videoTrack?.isEnabled = isVideoEnabled
    }

    func isCurrentVideoCapturer(_ capturer: RTCVideoCapturer) -> Bool {
        videoCapturer === capturer
    }
}

final class CallEngine {
    static let closedSignalSentTimeout: TimeInterval = 10

    let signaling: PeerConnectionSignaling & CallMessageEventProviding
    let role: CallRole
    let logger: LoggerProtocol
    let peerConnectionFactory: RTCPeerConnectionFactory
    let configFactory: WebRTCConfigMaking
    let purpose: String

    private var callType: ChatCallType

    private let stateSubject: AsyncCurrentValueSubject<CallEngineState>
    private let stateModel: CallEngineActor
    private let videoCaptureStrategy: VideoCaptureStrategyProtocol

    private var connectionTask: Task<Void, Never>?
    private var remoteCloseTask: Task<Void, Never>?
    private var offerDeliveryTask: Task<Void, Never>?
    private var iceFailureTask: Task<Void, Never>?
    private let remoteMediaStateSubject: AsyncCurrentValueSubject<CallRemoteMediaState>
    private let videoStateSubject: AsyncCurrentValueSubject<Bool>
    private let videoCaptureFailureSubject = AsyncPassthroughSubject<Void>()
    private var remoteMediaStateTask: Task<Void, Never>?

    var supportsAudio: Bool {
        #if targetEnvironment(simulator)
            // simulator doesn't support mic so we disable audio
            false
        #else
            true
        #endif
    }

    init(
        signaling: PeerConnectionSignaling & CallMessageEventProviding,
        role: CallRole,
        initialCallType: ChatCallType,
        purpose: String,
        configFactory: WebRTCConfigMaking,
        peerConnectionFactory: RTCPeerConnectionFactory,
        logger: LoggerProtocol
    ) {
        self.signaling = signaling
        self.role = role
        callType = initialCallType
        self.purpose = purpose
        self.configFactory = configFactory
        self.peerConnectionFactory = peerConnectionFactory
        self.logger = logger
        stateModel = CallEngineActor(isVideoEnabled: initialCallType == .video)

        videoCaptureStrategy = VideoCaptureStrategy(preferences: .init(profile: .call))

        switch role {
        case .initiator:
            stateSubject = .init(.contacting)
        case .acceptor:
            stateSubject = .init(.waiting)
        }

        remoteMediaStateSubject = .init(
            CallRemoteMediaState(isCameraEnabled: initialCallType == .video, isMicrophoneEnabled: true)
        )

        videoStateSubject = .init(initialCallType == .video)

        observeRemoteClose()
    }

    deinit {
        logger.debug("Deinit")
    }
}

private extension CallEngine {
    struct RemoteVideoTrackChange {
        let oldVideoTrack: RTCVideoTrack?
        let newVideoTrack: RTCVideoTrack?
        let renderer: RTCVideoRenderer?
    }

    struct RemoteRendererBinding {
        let oldRenderer: RTCVideoRenderer?
        let videoTrack: RTCVideoTrack?
    }
}

private extension CallEngine {
    func createTracks() -> CallTracks {
        let audioTrack = createAudioTrackIfNeeded()
        let videoTrack = createVideoTrack()

        return CallTracks(audioTrack: audioTrack, videoTrack: videoTrack)
    }

    func createAudioTrackIfNeeded() -> RTCAudioTrack? {
        guard supportsAudio else {
            return nil
        }

        let audioConstraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        let audioSource = peerConnectionFactory.audioSource(with: audioConstraints)
        return peerConnectionFactory.audioTrack(with: audioSource, trackId: "audio0")
    }

    func createVideoTrack() -> RTCVideoTrack {
        let videoSource = peerConnectionFactory.videoSource()
        return peerConnectionFactory.videoTrack(with: videoSource, trackId: "video0")
    }

    func makeDataConnectionCreator() -> DataConnectionCreating {
        switch role {
        case .initiator:
            DataConnectionInitiator(
                signaling: signaling,
                peerConnectionFactory: peerConnectionFactory,
                configFactory: configFactory,
                purpose: purpose,
                logger: logger
            )
        case .acceptor:
            DataConnectionAcceptor(
                signaling: signaling,
                peerConnectionFactory: peerConnectionFactory,
                configFactory: configFactory,
                logger: logger
            )
        }
    }

    func makeCallCreator(
        for dataConnected: PeerDataConnectionState.Connected,
        tracks: CallTracks
    ) -> CallCreatorProtocol {
        let transceiverConfigStrategy = TransceiverConfigStrategy(
            peerConnectionFactory: peerConnectionFactory,
            videoProfile: .call,
            logger: logger
        )

        switch role {
        case .initiator:
            return CallInitiator(
                connectionWrapper: dataConnected.connection,
                dataChannelWrapper: dataConnected.dataChannel,
                localTracks: tracks,
                transceiverConfigStrategy: transceiverConfigStrategy,
                logger: logger
            )
        case .acceptor:
            return CallAcceptor(
                connectionWrapper: dataConnected.connection,
                dataChannelWrapper: dataConnected.dataChannel,
                localTracks: tracks,
                transceiverConfigStrategy: transceiverConfigStrategy,
                logger: logger
            )
        }
    }

    func waitDataChannelConnected(
        from sequence: AnyAsyncSequence<PeerDataConnectionState>
    ) async -> PeerDataConnectionState.Connected? {
        do {
            for try await state in sequence {
                switch state {
                case .waiting:
                    stateSubject.send(.contacting)
                case .connecting:
                    stateSubject.send(.connecting)
                case let .connected(model):
                    return model
                case .disconnected:
                    stateSubject.send(.disconnected)
                    return nil
                }
            }

            return nil
        } catch {
            return nil
        }
    }

    func startStateReporting(with callCreator: CallCreatorProtocol) async {
        do {
            let sequence = callCreator.subscribeState()

            for try await state in sequence {
                guard !Task.isCancelled else {
                    return
                }

                switch state {
                case .creating:
                    logger.debug("Call: creating")

                    stateSubject.send(.connecting)
                case let .ready(model):
                    let hasAudio = model.audioTrack != nil
                    let hasVideo = model.videoTrack != nil

                    logger.debug("Call ready: audio=\(hasAudio), video=\(hasVideo)")

                    let videoTrackChange = await stateModel.setRemoteTracks(model)
                    await applyRemoteVideoTrackChange(videoTrackChange)

                    stateSubject.send(.connected)
                case .closed:
                    logger.debug("Call: closed")
                    stateSubject.send(.disconnected)
                }
            }
        } catch {
            logger.error("State reporting error: \(error)")
        }
    }

    func performConnection() async {
        let dataChannelCreator = makeDataConnectionCreator()

        observeOfferDelivery()

        guard let connectionStateSequence = try? await dataChannelCreator.connect() else {
            logger.error("Data channel creation failed")
            return
        }

        logger.debug("Establishing data channel...")

        guard let dataChannelConnected = await waitDataChannelConnected(from: connectionStateSequence) else {
            return
        }

        guard !Task.isCancelled else {
            return
        }

        logger.debug("Data channel established")

        await dataChannelCreator.throttle()

        let localTracks = createTracks()

        await stateModel.setLocalTracks(localTracks)
        await stateModel.setConnectionWrapper(dataChannelConnected.connection)

        observeIceFailure(on: dataChannelConnected.connection)

        let callCreator = makeCallCreator(for: dataChannelConnected, tracks: localTracks)

        await startMediaStateExchange(on: callCreator.multiplexedChannel)

        logger.debug("Upgrading to call")

        callCreator.setup()

        await startStateReporting(with: callCreator)

        logger.debug("Completed connection")

        RTCAudioSessionConfiguration.webRTC()
    }

    private func startVideoCaptureIfNeeded(from track: RTCVideoTrack) async -> Bool {
        guard let capturer = await stateModel.makeVideoCapturerIfNeeded(for: track) else {
            return true
        }

        do {
            switch capturer {
            case let fileCapturer as RTCFileVideoCapturer:
                fileCapturer.startCapturing(fromFileNamed: "test.mp4")
            case let cameraCapturer as RTCCameraVideoCapturer:
                try await startCameraVideoCapture(cameraCapturer)
            default:
                break
            }
        } catch {
            // Capture failure degrades the call to audio only: the caller rolls
            // video state back, the connection itself must stay up
            logger.error("Failed to start video capture: \(error)")
            await stateModel.clearVideoCapture(capturer)
            return false
        }

        if await !stateModel.isCurrentVideoCapturer(capturer) {
            capturer.stopAnyCapture()
        }

        return true
    }

    private func startLocalVideoCapture() async -> Bool {
        if let videoTrack = await stateModel.localTracks?.videoTrack {
            guard await startVideoCaptureIfNeeded(from: videoTrack) else {
                return false
            }
        }

        await stateModel.applyVideoEnabledState()

        return true
    }

    // The call survives a camera that won't start, but the user asked for video
    // and must learn it is off, so the rollback is reported alongside the state.
    private func handleVideoCaptureFailure() async {
        await disableVideo()

        videoCaptureFailureSubject.send(())
    }

    private func disableVideo() async {
        await stateModel.setVideoEnabled(false)
        await stateModel.applyVideoEnabledState()
        await stateModel.clearVideoCapture()

        await publishVideoState()
    }

    private func publishVideoState() async {
        let isEnabled = await stateModel.isVideoEnabled
        logger.debug("Video enabled: \(isEnabled)")

        videoStateSubject.send(isEnabled)
        await sendMediaState(.cameraEnabled(isEnabled))
    }

    private func startCameraVideoCapture(_ capturer: RTCCameraVideoCapturer) async throws {
        guard let frontCamera = (RTCCameraVideoCapturer.captureDevices().first { $0.position == .front }) else {
            throw CallEngineError.frontCameraUnavailable
        }

        guard let params = videoCaptureStrategy.deriveParams(for: frontCamera) else {
            throw CallEngineError.videoCaptureParamsUnavailable
        }

        try await capturer.startCapture(
            with: frontCamera,
            format: params.format,
            fps: params.fps
        )

        let dimensions = CMVideoFormatDescriptionGetDimensions(params.format.formatDescription)
        logger.debug("Video capture started: resolution: \(dimensions.width)x\(dimensions.height), fps: \(params.fps)")
    }

    func clearConnectionTask() {
        connectionTask?.cancel()
        connectionTask = nil
    }

    func clearRemoteCloseTask() {
        remoteCloseTask?.cancel()
        remoteCloseTask = nil
    }

    func clearOfferDeliveryTask() {
        offerDeliveryTask?.cancel()
        offerDeliveryTask = nil
    }

    func clearIceFailureTask() {
        iceFailureTask?.cancel()
        iceFailureTask = nil
    }

    func clearRemoteMediaStateTask() {
        remoteMediaStateTask?.cancel()
        remoteMediaStateTask = nil
    }

    func observeOfferDelivery() {
        guard role == .initiator else {
            return
        }

        let eventObserver = signaling

        offerDeliveryTask = Task { [weak self, eventObserver, logger] in
            do {
                for try await event in eventObserver.messageEvents {
                    guard !Task.isCancelled else { return }

                    if event == .offerDelivered {
                        await self?.handleOfferDelivered()
                        return
                    }
                }
            } catch {
                logger.error("Signal event observation failed: \(error)")
            }
        }
    }

    func observeIceFailure(on connection: AsyncPeerConnectionWrapper) {
        iceFailureTask = Task { [weak self, connection, logger] in
            let sequence = connection.iceConnectionState.eraseToAnyAsyncSequence()
            do {
                for try await state in sequence where state == .failed {
                    logger.debug("ICE connection failed")
                    self?.stateSubject.send(.failed)
                    return
                }
            } catch {
                logger.error("ICE failure observation failed: \(error)")
            }
        }
    }

    func observeRemoteClose() {
        remoteCloseTask = Task { [weak self, signaling, logger] in
            do {
                let signals = await signaling.signals

                for try await signal in signals {
                    guard !Task.isCancelled else { return }

                    if case .closed = signal {
                        logger.debug("Remote closed signal received; disconnecting")
                        self?.stateSubject.send(.disconnected)
                        return
                    }
                }
            } catch {
                logger.error("Remote close observation failed: \(error)")
            }
        }
    }

    func startMediaStateExchange(on multiplexedChannel: MultiplexedDataChannel) async {
        let channel = CallMediaStateChannel(multiplexedChannel: multiplexedChannel, logger: logger)
        await stateModel.setMediaStateChannel(channel)
        receiveRemoteMediaState(from: channel)

        let isVideoEnabled = await stateModel.isVideoEnabled

        if isVideoEnabled != (callType == .video) {
            await sendMediaState(.cameraEnabled(isVideoEnabled))
        }

        if await stateModel.isMuted {
            await sendMediaState(.microphoneEnabled(false))
        }
    }

    func receiveRemoteMediaState(from channel: CallMediaStateChannel) {
        remoteMediaStateTask = Task { [remoteMediaStateSubject, logger] in
            // The multiplexer back-pressures on send, so dropping this subscriber would stall
            // every use case on the connection. Only cancellation or the stream finishing
            // may end this loop.
            while !Task.isCancelled {
                do {
                    for try await signal in channel.signals {
                        logger.debug("Remote media state signal: \(signal)")
                        remoteMediaStateSubject.send(remoteMediaStateSubject.value.applying(signal))
                    }

                    return
                } catch {
                    logger.error("Remote media state observation failed, resuming: \(error)")
                }
            }
        }
    }

    func sendMediaState(_ signal: CallMediaStateSignal) async {
        guard let channel = await stateModel.mediaStateChannel else {
            return
        }

        do {
            try await channel.send(signal)
        } catch {
            logger.error("Media state send failed: \(error)")
        }
    }

    func handleOfferDelivered() async {
        guard role == .initiator else {
            return
        }

        guard stateSubject.value == .contacting else {
            return
        }

        logger.debug("Offer delivered, switching to waiting")
        stateSubject.send(.waiting)
    }

    func sendRemoteClosed() async -> Task<Void, Never>? {
        let closedSentTask = makeClosedSentTask()

        do {
            logger.debug("Sending closed message")
            try await signaling.send([.closed])
            logger.debug("Waiting for closedSent event")
            return closedSentTask
        } catch {
            closedSentTask.cancel()
            logger.error("Failed to send close signal: \(error)")
            return nil
        }
    }

    func makeClosedSentTask() -> Task<Void, Never> {
        Task { [signaling, logger] in
            try? await withTimeout(.seconds(Self.closedSignalSentTimeout)) {
                for try await event in signaling.messageEvents {
                    guard !Task.isCancelled else { return }
                    guard event == .closedSent else { continue }
                    logger.debug("Got closedSent event")
                    return
                }
            }
        }
    }

    @MainActor
    func applyRemoteVideoTrackChange(_ change: RemoteVideoTrackChange) {
        guard
            let renderer = change.renderer,
            change.oldVideoTrack !== change.newVideoTrack
        else {
            return
        }

        change.oldVideoTrack?.remove(renderer)
        change.newVideoTrack?.add(renderer)
    }
}

extension CallEngine: CallEngineProtocol {
    func observeState() -> AnyAsyncSequence<CallEngineState> {
        stateSubject.eraseToAnyAsyncSequence()
    }

    func connect() {
        connectionTask = Task { [weak self] in
            await self?.performConnection()
        }
    }

    func endCall(notifiesRemote: Bool) async {
        guard await stateModel.markEndedIfFirst() else {
            logger.debug("endCall ignored — already ended")
            return
        }
        logger.debug("Ending call")

        clearConnectionTask()
        clearRemoteCloseTask()
        clearOfferDeliveryTask()
        clearIceFailureTask()
        clearRemoteMediaStateTask()

        let closedSentTask = notifiesRemote ? await sendRemoteClosed() : nil

        await stateModel.clearVideoCapture()
        await stateModel.clearTracks()
        await stateModel.clearCallCreator()

        if let wrapper = await stateModel.connectionWrapper {
            await wrapper.close()
        }

        logger.debug("Cleanup done")

        if let closedSentTask {
            await closedSentTask.value
            logger.debug("Close sent")
        }
    }

    func attach(localRenderer: RTCVideoRenderer) {
        Task { @MainActor [weak self, stateModel] in
            guard let self else { return }

            let localTracks = await stateModel.localTracks
            let currentRenderer = await stateModel.localRenderer

            guard
                let videoTrack = localTracks?.videoTrack,
                currentRenderer !== localRenderer else {
                return
            }

            await stateModel.setLocalRenderer(localRenderer)

            if let currentRenderer {
                videoTrack.remove(currentRenderer)
            }

            videoTrack.add(localRenderer)

            if await !startVideoCaptureIfNeeded(from: videoTrack) {
                await handleVideoCaptureFailure()
            }
        }
    }

    func attach(remoteRenderer: RTCVideoRenderer) {
        Task { @MainActor [stateModel] in
            let rendererBinding = await stateModel.setRemoteRenderer(remoteRenderer)

            guard rendererBinding.oldRenderer !== remoteRenderer else {
                return
            }

            if let oldRenderer = rendererBinding.oldRenderer,
               let videoTrack = rendererBinding.videoTrack {
                videoTrack.remove(oldRenderer)
            }

            rendererBinding.videoTrack?.add(remoteRenderer)
        }
    }

    var isMuted: Bool {
        get async { await stateModel.isMuted }
    }

    func setMuted(_ isMuted: Bool) async -> Bool {
        let result = await stateModel.setMuted(isMuted)
        logger.debug("Audio muted: \(result)")
        await sendMediaState(.microphoneEnabled(!result))
        return result
    }

    var isVideoEnabled: Bool {
        get async { await stateModel.isVideoEnabled }
    }

    func setVideoEnabled(_ isEnabled: Bool) async {
        guard isEnabled else {
            await disableVideo()
            return
        }

        await stateModel.setVideoEnabled(true)

        if await startLocalVideoCapture() {
            await publishVideoState()
        } else {
            await handleVideoCaptureFailure()
        }
    }

    func observeRemoteMediaState() -> AnyAsyncSequence<CallRemoteMediaState> {
        remoteMediaStateSubject.eraseToAnyAsyncSequence()
    }

    func observeVideoState() -> AnyAsyncSequence<Bool> {
        videoStateSubject.eraseToAnyAsyncSequence()
    }

    func observeVideoCaptureFailure() -> AnyAsyncSequence<Void> {
        videoCaptureFailureSubject.eraseToAnyAsyncSequence()
    }
}
