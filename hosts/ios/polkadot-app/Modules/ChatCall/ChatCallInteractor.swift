import Foundation

final class ChatCallInteractor {
    static let terminalStateDwellSeconds: TimeInterval = 1.5

    weak var presenter: ChatCallInteractorOutputProtocol?

    let callKitManager: VoIPCallKitManaging
    let callEngine: CallEngineProtocol
    let logger: LoggerProtocol
    let role: CallRole
    let peer: CallPeer
    let callType: ChatCallType
    let audioSessionManager: CallAudioSessionManaging
    let backgroundTaskManager: CallBackgroundTaskManaging
    let operatingSystemMediator: OperatingSystemMediating
    let permissionsService: CallPermissionsServicing

    private var stateObserverTask: Task<Void, Never>?
    private var callKitAnswerTask: Task<Void, Never>?
    private var callKitEndTask: Task<Void, Never>?
    private var callKitMutedTask: Task<Void, Never>?
    private var audioRouteTask: Task<Void, Never>?
    private(set) var isEnding: Bool = false

    init(
        callKitManager: VoIPCallKitManaging = VoIPCallKitManager.shared,
        callEngine: CallEngineProtocol,
        audioSessionManager: CallAudioSessionManaging,
        backgroundTaskManager: CallBackgroundTaskManaging,
        operatingSystemMediator: OperatingSystemMediating,
        permissionsService: CallPermissionsServicing,
        role: CallRole,
        peer: CallPeer,
        callType: ChatCallType = .video,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.callKitManager = callKitManager
        self.callEngine = callEngine
        self.audioSessionManager = audioSessionManager
        self.operatingSystemMediator = operatingSystemMediator
        self.backgroundTaskManager = backgroundTaskManager
        self.permissionsService = permissionsService
        self.role = role
        self.peer = peer
        self.logger = logger
        self.callType = callType
        setupCallKit()
        observeCallEngineState()
    }

    deinit {
        callKitManager.markCallDisconnected(with: .remoteEnded)
        Task { [callEngine] in
            await callEngine.endCall(notifiesRemote: false)
        }
        logger.debug("Deinited")
    }
}

private extension ChatCallInteractor {
    func provideLocalRendererModel() async {
        switch callType {
        case .audio:
            let localModel = ChatCallRendererModel(attach: nil)
            await presenter?.didReceiveLocalRenderer(model: localModel)
        case .video:
            let localModel = ChatCallRendererModel { [weak callEngine] view in
                callEngine?.attach(localRenderer: view)
            }
            await presenter?.didReceiveLocalRenderer(model: localModel)
        }
    }

    func provideRemoteRendererModel() async {
        switch callType {
        case .audio:
            let remoteModel = ChatCallRendererModel(attach: nil)
            await presenter?.didReceiveRemoteRenderer(model: remoteModel)
        case .video:
            let remoteModel = ChatCallRendererModel { [weak callEngine] view in
                callEngine?.attach(remoteRenderer: view)
            }
            await presenter?.didReceiveRemoteRenderer(model: remoteModel)
        }
    }

    func setupCallKit() {
        callKitAnswerTask = Task { [weak self] in
            guard let sequence = self?.callKitManager.observeHasPendingAnswer() else {
                return
            }
            do {
                for try await hasPendingAnswer in sequence where hasPendingAnswer {
                    await self?.performAcceptCall(notifiesCallKit: false)
                }
            } catch {
                self?.logger.error("Pending answer task failure: \(error.localizedDescription)")
            }
        }

        callKitEndTask = Task { [weak self] in
            guard let sequence = self?.callKitManager.observeHasPendingEnd() else {
                return
            }
            do {
                for try await hasPendingEnd in sequence where hasPendingEnd {
                    Task {
                        await self?.performEndCall(notifiesCallKit: false, notifiesRemote: true)
                    }
                }
            } catch {
                self?.logger.error("Pending end task failure: \(error.localizedDescription)")
            }
        }

        callKitMutedTask = Task { [weak self] in
            guard let sequence = self?.callKitManager.observeMutedAction() else {
                return
            }
            do {
                for try await isMuted in sequence.compactMap({ $0 }) {
                    if isMuted {
                        await self?.setMuted(true, notifiesCallKit: false)
                    } else {
                        await self?.unmute(notifiesCallKit: false)
                    }
                }
            } catch {
                self?.logger.error("Pending muted task failure: \(error.localizedDescription)")
            }
        }
    }

    func cancelSubscriptions() {
        stateObserverTask?.cancel()
        stateObserverTask = nil

        callKitAnswerTask?.cancel()
        callKitAnswerTask = nil

        callKitEndTask?.cancel()
        callKitEndTask = nil

        callKitMutedTask?.cancel()
        callKitMutedTask = nil

        audioRouteTask?.cancel()
        audioRouteTask = nil
    }

    func observeCallEngineState() {
        stateObserverTask = Task { [weak self] in
            guard let sequence = self?.callEngine.observeState() else {
                return
            }

            do {
                for try await state in sequence {
                    guard let self else { return }
                    await handleCallEngineState(state)
                }
            } catch {
                self?.logger.error("State observation failed: \(error)")
            }
        }
    }

    @MainActor
    func handleCallEngineState(_ state: CallEngineState) async {
        switch state {
        case .contacting:
            logger.debug("Contacting peer...")
            presenter?.didUpdateCallState(.contacting)
        case .waiting:
            logger.debug("Call waiting, ringing...")
            presenter?.didUpdateCallState(.ringing)
        case .connecting:
            logger.debug("Connecting to a call...")
            callKitManager.markCallConnecting(for: role)
            presenter?.didUpdateCallState(.connecting)
        case .connected:
            logger.debug("Connected to call")
            callKitManager.markCallConnected(for: role)
            presenter?.didUpdateConnectedAt(Date())
            presenter?.didUpdateCallState(.connected)
            await provideLocalRendererModel()
            await provideRemoteRendererModel()
        case .disconnected:
            logger.debug("Disconnected from call")
            callKitManager.markCallDisconnected(with: .remoteEnded)
            Task {
                await self.performEndCall(
                    notifiesCallKit: false,
                    notifiesRemote: false,
                    terminalState: .ended
                )
            }
        case .failed:
            logger.debug("Call failed")
            callKitManager.markCallDisconnected(with: .failed)
            Task {
                await self.performEndCall(
                    notifiesCallKit: false,
                    notifiesRemote: false,
                    terminalState: .failed
                )
            }
        }
    }

    func makeCallKitInput() -> VoIPCallKitInput {
        .init(name: peer.name, callType: callType)
    }

    @MainActor
    func performSetup() async {
        guard role == .initiator else {
            return
        }

        guard await ensureCallPermissions() else {
            return
        }

        callKitManager.startOutgoingCall(with: makeCallKitInput())
        discoverCapabilities()
        callEngine.connect()
        setupAudioSession()
        operatingSystemMediator.disableScreenSleep()
    }

    @MainActor
    func performAcceptCall(notifiesCallKit: Bool) async {
        guard role == .acceptor else {
            return
        }

        let microphoneAccess = await permissionsService.resolveMicrophoneAccess(prompting: .whenActive)
        await permissionsService.requestCameraAccessIfNeeded(for: callType)

        if notifiesCallKit {
            callKitManager.answerFromAppOrEnsureStarted(with: makeCallKitInput())
        }

        await startMutedIfNeeded(for: microphoneAccess)

        discoverCapabilities()
        callEngine.connect()
        setupAudioSession()
        operatingSystemMediator.disableScreenSleep()
    }

    @MainActor
    func performEndCall(
        notifiesCallKit: Bool,
        notifiesRemote: Bool,
        terminalState: ChatCallState = .ended
    ) async {
        await performEndCall(
            notifiesCallKit: notifiesCallKit,
            notifiesRemote: notifiesRemote,
            terminalState: terminalState
        ) {
            presenter?.didEndCall()
        }
    }

    @MainActor
    func performEndCall(
        notifiesCallKit: Bool,
        notifiesRemote: Bool,
        terminalState: ChatCallState = .ended,
        reportOutcome: () -> Void
    ) async {
        guard !isEnding else {
            return
        }
        isEnding = true

        cancelSubscriptions()

        if notifiesCallKit {
            callKitManager.endFromApp()
        }

        operatingSystemMediator.enableScreenSleep()
        presenter?.didUpdateCallState(terminalState)

        let backgroundTaskManager = backgroundTaskManager
        let logger = logger

        if notifiesRemote {
            backgroundTaskManager.beginBackgroundTask()
        }

        defer {
            if notifiesRemote {
                backgroundTaskManager.endBackgroundTask()
            }
        }

        let endCallTask: Task<Void, Never> =
            if notifiesRemote {
                Task { [callEngine] in
                    await callEngine.endCall(notifiesRemote: true)
                }
            } else {
                Task { [callEngine] in
                    await callEngine.endCall(notifiesRemote: false)
                }
            }

        // dismiss call ui after small delay (to show final state)
        try? await Task.sleep(for: .seconds(Self.terminalStateDwellSeconds))
        reportOutcome()
        logger.debug("Outcome reported")

        await endCallTask.value
        logger.debug("Call ended")
    }

    @MainActor
    func ensureCallPermissions() async -> Bool {
        guard await permissionsService.ensurePermissions(for: callType) else {
            logger.warning("Microphone permission denied, ending the call")
            await performEndCall(notifiesCallKit: true, notifiesRemote: true)
            return false
        }

        return true
    }

    func setupAudioSession() {
        do {
            try audioSessionManager.configureAudioSession(for: callType)
            observeAudioRoute()
        } catch {
            logger.error("Can't configure audio session")
        }
    }

    func observeAudioRoute() {
        audioRouteTask?.cancel()
        audioRouteTask = Task { [weak self] in
            guard let sequence = self?.audioSessionManager.observeRouteState() else {
                return
            }
            do {
                for try await state in sequence {
                    await self?.presenter?.didUpdateAudioRoute(state)
                }
            } catch {
                self?.logger.error("Audio route task failure: \(error.localizedDescription)")
            }
        }
    }

    func discoverCapabilities() {
        Task { [weak self] in
            await self?.presenter?.didReceiveCapability([.mute, .audioRoute])
        }
    }
}

extension ChatCallInteractor: ChatCallInteractorInputProtocol {
    func setup() {
        Task {
            await performSetup()
        }
    }

    func acceptCall() {
        Task {
            await performAcceptCall(notifiesCallKit: true)
        }
    }

    func endCall() {
        Task {
            await performEndCall(notifiesCallKit: true, notifiesRemote: true)
        }
    }

    func toggleMute() {
        Task {
            if await callEngine.isMuted {
                await unmute(notifiesCallKit: true)
            } else {
                await setMuted(true, notifiesCallKit: true)
            }
        }
    }

    func selectAudioRoute(_ route: CallAudioRoute) {
        do {
            try audioSessionManager.selectRoute(route)
        } catch {
            logger.error("Failed to select audio route \(route): \(error)")
        }
    }
}
