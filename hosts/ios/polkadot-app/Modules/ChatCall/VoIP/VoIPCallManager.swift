// swiftlint:disable file_length
import AVFoundation
import CallKit
import PushKit
import AsyncExtensions
import KeyDerivation

protocol VoIPCallKitManaging: AnyObject {
    func reportIncomingCall(
        fromPushPayload payload: [AnyHashable: Any],
        completion: @escaping () -> Void
    )
    func startOutgoingCall(with input: VoIPCallKitInput)

    func markCallConnecting(for role: CallRole)
    func markCallConnected(for role: CallRole)
    func markCallDisconnected(with reason: CXCallEndedReason)

    func answerFromAppOrEnsureStarted(with input: VoIPCallKitInput)
    func endFromApp()

    func requestMutedFromApp(_ isMuted: Bool)
    func confirmMutedState(isSuccessful: Bool)

    func observeActiveCallData() -> AnyAsyncSequence<VoIPCallKitData?>
    func observeHasPendingAnswer() -> AnyAsyncSequence<Bool>
    func observeHasPendingEnd() -> AnyAsyncSequence<Bool>
    func observeMutedAction() -> AnyAsyncSequence<Bool?>
}

final class VoIPCallKitManager: NSObject {
    static let shared = VoIPCallKitManager()

    private let provider: CXProvider
    private let callController: CXCallController
    private let queue: DispatchQueue
    private let audioSessionManager: CallAudioSessionManaging
    private let contactsService: ContactsLocalStorageServicing
    private let messageCoder: ChatPushMessageCoding
    private let logger: LoggerProtocol

    private let activeCallDataSubject = AsyncCurrentValueSubject<VoIPCallKitData?>(nil)
    private let hasPendingAnswerSubject = AsyncCurrentValueSubject<Bool>(false)
    private let hasPendingEndSubject = AsyncCurrentValueSubject<Bool>(false)
    private let mutedActionSubject = AsyncCurrentValueSubject<Bool?>(nil)

    private var pendingStartAction: CXStartCallAction?
    private var pendingAnswerAction: CXAnswerCallAction?
    private var pendingEndAction: CXEndCallAction?
    private var pendingMutedAction: CXSetMutedCallAction?

    init(
        audioSessionManager: CallAudioSessionManaging = CallAudioSessionManager.shared,
        contactsService: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        messageCoder: ChatPushMessageCoding = ChatPushMessageCoder(
            encryptionManager: ChatEncryptionManager(
                entropyManager: RootEntropyManager.shared
            )
        ),
        queue: DispatchQueue = PushKitQueueProvider.queue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.audioSessionManager = audioSessionManager
        self.contactsService = contactsService
        self.messageCoder = messageCoder
        callController = CXCallController(queue: queue)

        let config = CXProviderConfiguration()
        config.includesCallsInRecents = false
        config.supportsVideo = true
        config.maximumCallGroups = 1
        config.maximumCallsPerCallGroup = 1
        config.supportedHandleTypes = [.generic]
        provider = CXProvider(configuration: config)
        self.queue = queue
        self.logger = logger
        super.init()
        provider.setDelegate(self, queue: queue)
    }
}

// MARK: - VoIPCallKitManaging

extension VoIPCallKitManager: VoIPCallKitManaging {
    func reportIncomingCall(
        fromPushPayload payload: [AnyHashable: Any],
        completion: @escaping () -> Void
    ) {
        dispatchPrecondition(condition: .onQueue(queue))

        guard activeCallData == nil else {
            // This is a hack to prevent iOS to throttle voIP pushes if we
            // already have an active call and do not call `reportNewIncomingCall`.
            // For iOS 26.4+, we should implement the new logic based on the `mustReport`
            // property described here https://developer.apple.com/documentation/pushkit/pkpushregistrydelegate/pushregistry(_:didreceiveincomingvoippushwith:metadata:withcompletionhandler:)
            performReportBusyIncomingCall(completion: completion)
            return
        }

        let uuid = UUID()
        logger.debug("Reporting incoming call \(uuid)")

        activeCallData = .init(
            uuid: uuid,
            status: .initiallyReported,
            pendingAnswerOrEndActionSource: nil,
            pendingMutedActionSource: nil
        )

        provider.reportNewIncomingCall(with: uuid, update: makeInitialIncomingCallUpdate()) { [weak self] error in
            completion()

            if let error {
                self?.logger.error("Could not report incoming call \(uuid): \(error.localizedDescription)")
                self?.activeCallData = nil
            } else {
                self?.logger.debug("Call initially reported \(uuid)")
                self?.updateReportedCall(with: uuid, fromPushPayload: payload)
            }
        }
    }

    func startOutgoingCall(with input: VoIPCallKitInput) {
        queue.async { [weak self] in
            self?.performStartOutgoingCall(with: input)
        }
    }

    func markCallConnecting(for role: CallRole) {
        queue.async { [weak self] in
            switch role {
            case .acceptor:
                break
            case .initiator:
                self?.performMarkOutgoingCallConnecting()
            }
        }
    }

    func markCallConnected(for role: CallRole) {
        queue.async { [weak self] in
            switch role {
            case .acceptor:
                self?.performMarkIncomingCallConnected()
            case .initiator:
                self?.performMarkOutgoingCallConnected()
            }
        }
    }

    func markCallDisconnected(with reason: CXCallEndedReason) {
        queue.async { [weak self] in
            self?.fulfillPendingStartAction(isSuccessful: false)
            self?.fulfillPendingAnswerAction(isSuccessful: false)
            self?.fulfillPendingEndAction()
            self?.fulfillPendingMutedAction(isSuccessful: false)

            guard let uuid = self?.activeCallData?.uuid else {
                self?.logger.debug("No active call")
                return
            }

            self?.performReportCallEnd(with: uuid, with: reason)
        }
    }

    func answerFromAppOrEnsureStarted(with input: VoIPCallKitInput) {
        queue.async { [weak self] in
            if let data = self?.activeCallData {
                self?.performAnswerFromApp(with: data)
            } else {
                self?.performEnsureIncomingCallStarted(with: input)
            }
        }
    }

    func endFromApp() {
        queue.async { [weak self] in
            guard let data = self?.activeCallData else {
                self?.logger.debug("No active call")
                return
            }

            let uuid = data.uuid
            self?.activeCallData = data.updatingPendingAnswerOrEndActionSource(.app)

            let action = CXEndCallAction(call: uuid)
            let transaction = CXTransaction(action: action)

            self?.callController.request(transaction) { error in
                if let error {
                    self?.logger.error("Could not answer from app \(uuid): \(error.localizedDescription)")
                    self?.activeCallData = data
                } else {
                    self?.logger.debug("Call ended from app \(uuid)")
                }
            }
        }
    }

    func requestMutedFromApp(_ isMuted: Bool) {
        queue.async { [weak self] in
            guard let data = self?.activeCallData else {
                self?.logger.debug("No active call")
                return
            }

            let uuid = data.uuid
            self?.activeCallData = data.updatingPendingMutedActionSource(.app)

            let action = CXSetMutedCallAction(call: uuid, muted: isMuted)
            let transaction = CXTransaction(action: action)

            self?.callController.request(transaction) { error in
                if let error {
                    self?.logger.error("Could not set muted from app \(uuid): \(error.localizedDescription)")
                    self?.activeCallData = data
                } else {
                    self?.logger.debug("Muted \(isMuted) requested from app \(uuid)")
                }
            }
        }
    }

    func confirmMutedState(isSuccessful: Bool) {
        queue.async { [weak self] in
            guard let data = self?.activeCallData else {
                return
            }
            self?.fulfillPendingMutedAction(isSuccessful: isSuccessful)
            self?.activeCallData = data.updatingPendingMutedActionSource(nil)
        }
    }

    func observeActiveCallData() -> AnyAsyncSequence<VoIPCallKitData?> {
        activeCallDataSubject.eraseToAnyAsyncSequence()
    }

    func observeHasPendingAnswer() -> AnyAsyncSequence<Bool> {
        hasPendingAnswerSubject.eraseToAnyAsyncSequence()
    }

    func observeHasPendingEnd() -> AnyAsyncSequence<Bool> {
        hasPendingEndSubject.eraseToAnyAsyncSequence()
    }

    func observeMutedAction() -> AnyAsyncSequence<Bool?> {
        mutedActionSubject.eraseToAnyAsyncSequence()
    }
}

// MARK: - CXProviderDelegate

extension VoIPCallKitManager: CXProviderDelegate {
    func providerDidReset(_: CXProvider) {
        logger.debug("Provider did reset")
        clearState()
    }

    func provider(_: CXProvider, perform action: CXStartCallAction) {
        logger.debug("Call started \(action.uuid)")

        guard
            let callData = activeCallData,
            action.callUUID == callData.uuid
        else {
            action.fail()
            return
        }

        pendingStartAction = action
    }

    func provider(_: CXProvider, perform action: CXAnswerCallAction) {
        logger.debug("Did answer call \(action.callUUID)")

        guard
            let callData = activeCallData,
            action.callUUID == callData.uuid
        else {
            action.fail()
            return
        }

        pendingAnswerAction = action

        // Do not notify if action is sent via app
        guard callData.pendingAnswerOrEndActionSource != .app else {
            return
        }

        activeCallData = callData.updatingPendingAnswerOrEndActionSource(.system)
        hasPendingAnswerSubject.send(true)
    }

    func provider(_: CXProvider, perform action: CXEndCallAction) {
        logger.debug("Did end call \(action.callUUID)")

        guard
            let callData = activeCallData,
            action.callUUID == callData.uuid
        else {
            action.fail()
            return
        }

        pendingEndAction = action

        // Do not notify if action is sent via app
        guard callData.pendingAnswerOrEndActionSource != .app else {
            return
        }

        activeCallData = callData.updatingPendingAnswerOrEndActionSource(.system)
        hasPendingEndSubject.send(true)
    }

    func provider(_: CXProvider, perform action: CXSetMutedCallAction) {
        logger.debug("Muted action \(action.callUUID): \(action.isMuted)")

        guard
            let callData = activeCallData,
            action.callUUID == callData.uuid
        else {
            action.fail()
            return
        }

        pendingMutedAction = action

        // Do not notify if action is sent via app
        guard callData.pendingMutedActionSource != .app else {
            return
        }

        activeCallData = callData.updatingPendingMutedActionSource(.system)
        mutedActionSubject.send(action.isMuted)
    }

    func provider(_: CXProvider, didActivate session: AVAudioSession) {
        audioSessionManager.setEnabled(true, for: session)
    }

    func provider(_: CXProvider, didDeactivate session: AVAudioSession) {
        audioSessionManager.setEnabled(false, for: session)
    }
}

// MARK: - Private

private extension VoIPCallKitManager {
    enum InputError: Error {
        case invalidMessageData
        case noContact
        case unsupportedMessage
        case blockedContact
    }

    var activeCallData: VoIPCallKitData? {
        get { activeCallDataSubject.value }
        set { activeCallDataSubject.send(newValue) }
    }

    func updateReportedCall(with uuid: UUID, fromPushPayload payload: [AnyHashable: Any]) {
        Task {
            do {
                let input = try await makeInput(pushPayload: payload)
                updateReportedCall(with: uuid, input: input)
            } catch {
                logger.error("Failed to report update for the call \(uuid): \(error.localizedDescription)")
                reportCallEnd(with: .failed)
            }
        }
    }

    func updateReportedCall(with uuid: UUID, input: VoIPCallKitInput) {
        let update = makeCallUpdate(input: input)

        queue.async { [weak self] in
            self?.logger.debug("Reporting update for the call \(uuid)")
            self?.provider.reportCall(with: uuid, updated: update)
            self?.activeCallData = .init(
                uuid: uuid,
                status: .reported(input),
                pendingAnswerOrEndActionSource: nil,
                pendingMutedActionSource: nil
            )
        }
    }

    func makeInitialIncomingCallUpdate() -> CXCallUpdate {
        let update = CXCallUpdate()
        update.hasVideo = false
        update.localizedCallerName = .init(localized: .callKitManagerInitialName)
        update.supportsDTMF = false
        update.supportsHolding = false
        update.supportsGrouping = false
        update.supportsUngrouping = false
        update.remoteHandle = CXHandle(
            type: .generic,
            value: .init(localized: .callKitManagerInitialHandle)
        )
        return update
    }

    func makeCallUpdate(input: VoIPCallKitInput) -> CXCallUpdate {
        let update = makeInitialIncomingCallUpdate()
        update.remoteHandle = CXHandle(type: .generic, value: input.name)
        update.localizedCallerName = input.name
        update.hasVideo = input.hasVideo
        return update
    }

    func makeInput(pushPayload: [AnyHashable: Any]) async throws -> VoIPCallKitInput {
        guard let pushId = pushPayload[
            PushNotificationKeys.pushId
        ] as? String else {
            throw InputError.invalidMessageData
        }

        guard let messageHex = pushPayload[
            PushNotificationKeys.message
        ] as? String else {
            throw InputError.invalidMessageData
        }

        let contact = try await contactsService.getContact(byPushId: pushId)
            .asyncExecute()
            .mapOrThrow(InputError.noContact)

        guard !contact.isBlocked else {
            throw InputError.blockedContact
        }

        let payload = try messageCoder.decodeMessage(messageHex, for: contact)

        guard let offer = payload.dataChannelOffer else {
            throw InputError.unsupportedMessage
        }

        return VoIPCallKitInput(
            name: contact.username,
            callType: .init(remoteType: offer.purpose)
        )
    }

    func reportCallEnd(with reason: CXCallEndedReason) {
        queue.async { [weak self] in
            guard let uuid = self?.activeCallData?.uuid else {
                self?.logger.debug("No active call")
                return
            }
            self?.performReportCallEnd(with: uuid, with: reason)
        }
    }

    func performReportBusyIncomingCall(completion: @escaping () -> Void) {
        let uuid = UUID()
        let update = makeInitialIncomingCallUpdate()
        update.localizedCallerName = .init(localized: .callKitManagerBusy)
        update.remoteHandle = .init(type: .generic, value: .init(localized: .callKitManagerBusy))

        logger.debug("Reporting busy incoming call \(uuid)")

        provider.reportNewIncomingCall(with: uuid, update: update) { [weak self] error in
            completion()

            if let error {
                self?.logger.debug("Reporting busy call \(uuid) failed: \(error.localizedDescription)")
            } else {
                self?.logger.debug("Reported busy call \(uuid), reporting end immediately")
                self?.provider.reportCall(with: uuid, endedAt: Date(), reason: .failed)
            }
        }
    }

    func performReportCallEnd(with uuid: UUID, with reason: CXCallEndedReason) {
        logger.debug("Reporting call end \(uuid)")
        provider.reportCall(with: uuid, endedAt: Date(), reason: reason)
        clearState()
    }

    func performAnswerFromApp(with data: VoIPCallKitData) {
        let uuid = data.uuid
        activeCallData = data.updatingPendingAnswerOrEndActionSource(.app)

        let action = CXAnswerCallAction(call: uuid)
        let transaction = CXTransaction(action: action)

        callController.request(transaction) { [weak self] error in
            if let error {
                self?.logger.error("Could not answer from app \(uuid): \(error.localizedDescription)")
                self?.activeCallData = data
            } else {
                self?.logger.debug("Call answered from app \(uuid)")
            }
        }
    }

    func performEnsureIncomingCallStarted(with input: VoIPCallKitInput) {
        let uuid = UUID()
        let handle = CXHandle(type: .generic, value: input.name)
        let action = CXStartCallAction(call: uuid, handle: handle)
        action.isVideo = input.hasVideo

        let transaction = CXTransaction(action: action)

        logger.debug("Starting incoming call \(uuid)")

        activeCallData = .init(
            uuid: uuid,
            status: .reported(input),
            pendingAnswerOrEndActionSource: nil,
            pendingMutedActionSource: nil
        )

        callController.request(transaction) { [weak self] error in
            if let error {
                self?.logger.error("Could not start incoming call \(uuid): \(error.localizedDescription)")
                self?.activeCallData = nil
            } else {
                self?.logger.debug("Call started \(uuid)")
            }
        }
    }

    func performStartOutgoingCall(with input: VoIPCallKitInput) {
        if let uuid = activeCallData?.uuid {
            performReportCallEnd(with: uuid, with: .failed)
        }
        clearState()

        let uuid = UUID()
        let handle = CXHandle(type: .generic, value: input.name)
        let action = CXStartCallAction(call: uuid, handle: handle)
        action.isVideo = input.hasVideo

        let transaction = CXTransaction(action: action)

        logger.debug("Starting outgoing call \(uuid)")

        activeCallData = .init(
            uuid: uuid,
            status: .reported(input),
            pendingAnswerOrEndActionSource: nil,
            pendingMutedActionSource: nil
        )

        callController.request(transaction) { [weak self] error in
            if let error {
                self?.logger.error("Could not start outgoing call \(uuid): \(error.localizedDescription)")
                self?.activeCallData = nil
            } else {
                self?.logger.debug("Call started \(uuid)")
            }
        }
    }

    func performMarkOutgoingCallConnecting() {
        guard let uuid = activeCallData?.uuid else {
            logger.debug("No active call")
            return
        }
        provider.reportOutgoingCall(with: uuid, startedConnectingAt: Date())
    }

    func performMarkIncomingCallConnected() {
        guard
            let callData = activeCallData,
            let input = callData.status.input
        else {
            logger.debug("No active call")
            return
        }

        logger.debug("Reporting incoming call as connected \(callData.uuid)")
        fulfillPendingStartAction(isSuccessful: true)
        fulfillPendingAnswerAction(isSuccessful: true)

        activeCallData = .init(
            uuid: callData.uuid,
            status: .connected(input),
            pendingAnswerOrEndActionSource: nil,
            pendingMutedActionSource: nil
        )
    }

    func performMarkOutgoingCallConnected() {
        guard
            let callData = activeCallData,
            let input = callData.status.input
        else {
            logger.debug("No active call")
            return
        }

        logger.debug("Reporting outgoing call as connected \(callData.uuid)")
        fulfillPendingStartAction(isSuccessful: true)
        provider.reportOutgoingCall(with: callData.uuid, connectedAt: Date())

        activeCallData = .init(
            uuid: callData.uuid,
            status: .connected(input),
            pendingAnswerOrEndActionSource: nil,
            pendingMutedActionSource: nil
        )
    }

    func fulfillPendingStartAction(isSuccessful: Bool) {
        guard let action = pendingStartAction else {
            return
        }
        if isSuccessful {
            action.fulfill()
        } else {
            action.fail()
        }
        pendingStartAction = nil
    }

    func fulfillPendingAnswerAction(isSuccessful: Bool) {
        guard let action = pendingAnswerAction else {
            return
        }
        if isSuccessful {
            action.fulfill()
        } else {
            action.fail()
        }
        pendingAnswerAction = nil
        hasPendingAnswerSubject.send(false)
    }

    func fulfillPendingEndAction() {
        guard let action = pendingEndAction else {
            return
        }
        action.fulfill()
        pendingEndAction = nil
        hasPendingEndSubject.send(false)
    }

    func fulfillPendingMutedAction(isSuccessful: Bool) {
        guard let action = pendingMutedAction else {
            return
        }
        if isSuccessful {
            action.fulfill()
        } else {
            action.fail()
        }
        pendingMutedAction = nil
        mutedActionSubject.send(nil)
    }

    func clearState() {
        activeCallData = nil

        pendingStartAction = nil
        pendingAnswerAction = nil
        pendingEndAction = nil
        pendingMutedAction = nil

        hasPendingAnswerSubject.send(false)
        hasPendingEndSubject.send(false)
        mutedActionSubject.send(nil)
    }
}

// swiftlint:enable file_length
