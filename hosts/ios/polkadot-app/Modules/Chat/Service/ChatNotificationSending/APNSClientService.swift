import Foundation
import Operation_iOS
import MessageExchangeKit
import StructuredConcurrency

protocol APNSClientServicing {
    func notifyAboutNewMessageWrapper(
        _ message: Chat.RemoteMessage,
        contact: Chat.Contact
    ) -> CompoundOperationWrapper<NotifyResponse>
}

final class APNSClientService {
    private let pushIdFactory: ChatPushIdMaking
    private let messageCoder: ChatPushMessageCoding
    private let payloadBuilder: ChatNotificationPayloadBuilding
    private let tokenProvider: JWTTokenProviding
    private let workQueue: DispatchQueue
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        pushIdFactory: ChatPushIdMaking,
        messageCoder: ChatPushMessageCoding,
        payloadBuilder: ChatNotificationPayloadBuilding,
        tokenProvider: JWTTokenProviding,
        workQueue: DispatchQueue,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.pushIdFactory = pushIdFactory
        self.messageCoder = messageCoder
        self.payloadBuilder = payloadBuilder
        self.tokenProvider = tokenProvider
        self.workQueue = workQueue
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension APNSClientService: APNSClientServicing {
    func notifyAboutNewMessageWrapper(
        _ message: Chat.RemoteMessage,
        contact: Chat.Contact
    ) -> CompoundOperationWrapper<NotifyResponse> {
        guard let pushId = pushIdFactory.makePushId(
            peer: contact.toMessageExchangePeer(),
            own: contact.ownKeyId.toMessageExchangeOwn()
        ) else {
            logger.error("Missing push id")
            return .createWithError(NotifyError.missingPushId)
        }

        return wrapper(for: message, contact: contact, pushId: pushId)
    }
}

private extension APNSClientService {
    enum NotifyError: Error {
        case missingPushId
        case missingPushToken
        case missingPeerPlatform
        case missingBundleId
        case notNotifiableMessageType
    }

    func wrapper(
        for message: Chat.RemoteMessage,
        contact: Chat.Contact,
        pushId: Chat.PushId
    ) -> CompoundOperationWrapper<NotifyResponse> {
        let parametersOperation = parametersOperation(
            for: message,
            contact: contact,
            pushId: pushId
        )

        return notifyOperation(parametersOperation: parametersOperation)
    }

    func parametersOperation(
        for message: Chat.RemoteMessage,
        contact: Chat.Contact,
        pushId: Chat.PushId
    ) -> BaseOperation<NotifyRequestParameters> {
        ClosureOperation { [weak self] in
            guard let self else {
                throw BaseOperationError.unexpectedDependentResult
            }

            guard message.supportsNotification() else {
                throw NotifyError.notNotifiableMessageType
            }

            let isVoIP = shouldSendVoIPPush(for: contact, message: message)

            let tokenString = isVoIP
                ? makeVoIPPushTokenString(contact: contact)
                : makePushTokenString(contact: contact)

            guard let tokenString else {
                throw NotifyError.missingPushToken
            }

            guard let platform = contact.peerPlatform else {
                throw NotifyError.missingPeerPlatform
            }

            guard let bundleId = Bundle.main.bundleIdentifier else {
                throw NotifyError.missingBundleId
            }

            let payload = try payloadBuilder.makePayload(for: message)

            return try NotifyRequestParameters(
                deviceToken: tokenString,
                pushId: pushId.ownString,
                bundlerId: bundleId,
                platform: platform.rawValue,
                message: messageCoder.encodeMessage(payload, for: contact),
                voip: isVoIP
            )
        }
    }

    func notifyOperation(
        parametersOperation: BaseOperation<NotifyRequestParameters>
    ) -> CompoundOperationWrapper<NotifyResponse> {
        let operation = AsyncTaskOperation<NotifyResponse> { [tokenProvider] in
            let parameters = try parametersOperation.extractNoCancellableResultData()

            return try await tokenProvider.withAuthorizedToken { token in
                let endpoint = UsernameApi.V1.notify(parameters)
                let requestFactory = BlockNetworkRequestFactory {
                    var request = URLRequest(url: endpoint.url)
                    request.httpMethod = endpoint.httpMethod
                    request.setValue(
                        HttpContentType.json.rawValue,
                        forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
                    )

                    let modifier = BearerTokenRequestModifier(token: token)
                    try modifier.visit(request: &request)

                    if let bodyParams = endpoint.params {
                        request.httpBody = try JSONEncoder().encode(bodyParams)
                    }

                    return request
                }

                let networkOp = NetworkOperation(
                    requestFactory: requestFactory,
                    resultFactory: AnyNetworkResultFactory(
                        factory: UsernameJsonResultFactory<NotifyResponse>()
                    )
                )

                return try await networkOp.asyncExecute()
            }
        }

        operation.addDependency(parametersOperation)

        return CompoundOperationWrapper(
            targetOperation: operation,
            dependencies: [parametersOperation]
        )
    }

    func shouldSendVoIPPush(for contact: Chat.Contact, message: Chat.RemoteMessage) -> Bool {
        let supportsVoIP = message.isVoIPNotification() && contact.supportsVoIPPushes
        let hasVoIPToken = makeVoIPPushTokenString(contact: contact) != nil
        return supportsVoIP && hasVoIPToken
    }

    func makePushTokenString(contact: Chat.Contact) -> String? {
        guard
            let token = contact.pushToken,
            let platform = contact.peerPlatform
        else {
            return nil
        }
        switch platform {
        case .android:
            return String(data: token, encoding: .utf8)
        case .ios:
            return token.toHex()
        }
    }

    func makeVoIPPushTokenString(contact: Chat.Contact) -> String? {
        guard let token = contact.voipPushToken else {
            return nil
        }
        return token.toHex()
    }
}
