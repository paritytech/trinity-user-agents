import Foundation
import KeyDerivation
import NovaCrypto
import SubstrateSdk

public typealias RenderWidgetHandler = (_ messageId: String, _ scaleHex: String) -> Void

public enum ContainerBridgeHostApiError: Error {
    case missingRequiredParam(String)
    case invalidSignRawParams

    public var errorDescription: String? {
        switch self {
        case let .missingRequiredParam(param):
            "missing required param \(param)"
        case .invalidSignRawParams:
            "signRaw must have either data or payload"
        }
    }
}

public extension ContainerBridge {
    /// Register all host API handlers on the bridge.
    /// Must be called before evaluating container.js.
    func registerHostApiHandlers(
        nativeApi: ProductsNativeApiProtocol,
        onRenderWidget: @escaping RenderWidgetHandler
    ) {
        registerAccountGet(nativeApi: nativeApi)
        registerAccountGetAlias(nativeApi: nativeApi)
        registerAccountCreateProof(nativeApi: nativeApi)
        registerAccountSignVrf(nativeApi: nativeApi)
        registerGetUserId(nativeApi: nativeApi)
        registerChainNodes(nativeApi: nativeApi)
        registerChainSupported(nativeApi: nativeApi)
        registerChatSendTextMessage(nativeApi: nativeApi)
        registerChatSendCustomMessage(nativeApi: nativeApi)
        registerChatCreateRoom(nativeApi: nativeApi)
        registerChatSubscribeRooms(nativeApi: nativeApi)
        registerChatRenderWidget(onRenderWidget: onRenderWidget)
        registerGetNonProductAccounts(nativeApi: nativeApi)
        registerSignPayload(nativeApi: nativeApi)
        registerSignPayloadLegacy(nativeApi: nativeApi)
        registerSignRaw(nativeApi: nativeApi)
        registerSignRawLegacy(nativeApi: nativeApi)
        registerCreateTransaction(nativeApi: nativeApi)
        registerCreateTransactionLegacy(nativeApi: nativeApi)
        registerLocalStorageRead(nativeApi: nativeApi)
        registerLocalStorageWrite(nativeApi: nativeApi)
        registerLocalStorageClear(nativeApi: nativeApi)
        registerLocalStorageSubscribe(nativeApi: nativeApi)
        registerWorkerBeginOperation(nativeApi: nativeApi)
        registerWorkerEndOperation(nativeApi: nativeApi)
        registerNavigateTo(nativeApi: nativeApi)
        registerAllowNetworkAccess(nativeApi: nativeApi)
        registerAllowWebRtcAccess(nativeApi: nativeApi)
        registerStatementStoreSubscribe(nativeApi: nativeApi)
        registerStatementStoreCreateProof(nativeApi: nativeApi)
        registerStatementStoreCreateProofAuthorized(nativeApi: nativeApi)
        registerStatementStoreSubmit(nativeApi: nativeApi)
        registerPreimageLookup(nativeApi: nativeApi)
        registerPreimageSubmit(nativeApi: nativeApi)
        registerDevicePermission(nativeApi: nativeApi)
        registerRemotePermission(nativeApi: nativeApi)
        registerPaymentBalanceSubscribe(nativeApi: nativeApi)
        registerPaymentTopUp(nativeApi: nativeApi)
        registerPaymentTopUpStatusSubscribe(nativeApi: nativeApi)
        registerHostPaymentRequest(nativeApi: nativeApi)
        registerHostPaymentStatusSubscribe(nativeApi: nativeApi)
        registerPushNotification(nativeApi: nativeApi)
        registerCancelPushNotification(nativeApi: nativeApi)
        registerJSDeviceCapability(nativeApi: nativeApi)
        registerDeriveEntropy(nativeApi: nativeApi)
        registerRequestResourceAllocation(nativeApi: nativeApi)
        registerThemeSubscribe(nativeApi: nativeApi)
        registerLocaleSubscribe(nativeApi: nativeApi)
    }
}

// MARK: - Theme

private extension ContainerBridge {
    func registerThemeSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "themeSubscribe") { _ in
            await nativeApi.subscribeTheme()
                .map { theme in
                    JSON.dictionaryValue([
                        "name": .stringValue(theme.name),
                        "variant": .stringValue(theme.variant.rawValue)
                    ])
                }
                .eraseToAnyAsyncSequence()
        }
    }
}

// MARK: - Locale

private extension ContainerBridge {
    func registerLocaleSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "localeSubscribe") { _ in
            nativeApi.subscribeLocale()
                .map { tag in
                    JSON.dictionaryValue(["languageTag": .stringValue(tag)])
                }
                .eraseToAnyAsyncSequence()
        }
    }
}

// MARK: - Account

private extension ContainerBridge {
    func registerAccountGet(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "accountGet") { params in
            let accountId = try params.mapOrMissing(for: "account") { try? $0.map(to: ProductAccountId.self) }

            let result = try await nativeApi.accountGet(accountId)

            return JSON.dictionaryValue(
                [
                    "publicKey": JSON.stringValue(result.publicKey)
                ]
            )
        }
    }

    func registerGetUserId(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "getUserId") { _ in
            let result = try await nativeApi.getUserId()

            return JSON.dictionaryValue(
                [
                    "primaryUsername": JSON.stringValue(result.primaryUsername)
                ]
            )
        }
    }

    func registerGetNonProductAccounts(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "getNonProductAccounts") { _ in
            let accounts = try await nativeApi.getNonProductAccounts()

            return JSON.arrayValue(
                accounts.map { account in
                    JSON.dictionaryValue(
                        [
                            "publicKey": JSON.stringValue(account.publicKey),
                            "name": account.name.map(JSON.stringValue) ?? JSON.null
                        ]
                    )
                }
            )
        }
    }

    func registerAccountGetAlias(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "accountGetAlias") { params in
            let request = try params.map(to: AccountGetAliasRequest.self)

            let result = try await nativeApi.accountGetAlias(
                context: request.context,
                ring: request.ring
            )

            return try result.toScaleCompatibleJSON()
        }
    }

    func registerAccountCreateProof(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "accountCreateProof") { params in
            let request = try params.map(to: AccountCreateProofRequest.self)

            let result = try await nativeApi.accountCreateProof(
                context: request.context,
                ring: request.ring,
                message: request.message
            )

            return try result.toScaleCompatibleJSON()
        }
    }

    func registerAccountSignVrf(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "accountSignVrf") { params in
            let payload = try params.map(to: SignVrfPayload.self)

            let signature = try await nativeApi.signVrf(payload)

            return try SignVrfResult(signature: signature).toScaleCompatibleJSON()
        }
    }
}

private struct AccountGetAliasRequest: Decodable {
    let context: ProductProofContext
    let ring: RingLocation
}

private struct AccountCreateProofRequest: Decodable {
    let context: ProductProofContext
    let ring: RingLocation
    @HexCodable var message: Data
}

// MARK: - Chain

private extension ContainerBridge {
    func registerChainNodes(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "chainNodes") { params in
            let genesisHash = try params.mapOrMissing(for: "genesisHash") { $0.stringValue }
            let nodes = try await nativeApi.chainNodes(genesisHash: genesisHash)

            return JSON.arrayValue(
                nodes.map(JSON.stringValue)
            )
        }
    }

    func registerChainSupported(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "chainSupported") { params in
            let genesisHash = try params.mapOrMissing(for: "genesisHash") { $0.stringValue }
            let isSupported = try await nativeApi.chainSupported(genesisHash: genesisHash)
            return JSON.boolValue(isSupported)
        }
    }
}

// MARK: - Chat Messages

private extension ContainerBridge {
    func registerChatSendTextMessage(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "chatSendTextMessage") { params in
            let text = try params.mapOrMissing(for: "text") { $0.stringValue }
            let roomId = params["chatId"]?.stringValue
            let messageId = try await nativeApi.sendMessage(.text(text), roomId: roomId)
            return JSON.dictionaryValue(["messageId": .stringValue(messageId)])
        }
    }

    func registerChatSendCustomMessage(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "chatSendCustomMessage") { params in
            let messageType = try params.mapOrMissing(for: "messageType") { $0.stringValue }
            let payloadHex = try params.mapOrMissing(for: "payloadHex") { $0.stringValue }
            let roomId = params["chatId"]?.stringValue

            let data = try Data(hexString: payloadHex)
            let messageId = try await nativeApi.sendMessage(
                .custom(messageType: messageType, data: data, alt: nil),
                roomId: roomId
            )
            return JSON.dictionaryValue(["messageId": JSON.stringValue(messageId)])
        }
    }

    func registerChatCreateRoom(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "chatCreateRoom") { params in
            let roomId = try params.mapOrMissing(for: "roomId") { $0.stringValue }
            let name = params["name"]?.stringValue
            let icon = params["icon"]?.stringValue

            let request = CreateRoomRequest(roomId: roomId, name: name, icon: icon)
            let result = try await nativeApi.createRoom(request)

            return JSON.dictionaryValue(["status": .stringValue(result.status.rawValue)])
        }
    }

    func registerChatSubscribeRooms(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "chatSubscribeRooms") { _ in
            let source = try await nativeApi.subscribeRooms()

            return source
                .map { rooms -> JSON in
                    let jsonRooms = rooms.map { room in
                        JSON.dictionaryValue([
                            "roomId": .stringValue(room.roomId),
                            "participatingAs": .stringValue(room.participation.rawValue)
                        ])
                    }

                    return JSON.arrayValue(jsonRooms)
                }
                .eraseToAnyAsyncSequence()
        }
    }

    func registerChatRenderWidget(onRenderWidget: @escaping RenderWidgetHandler) {
        registerRequestHandler(method: "chatRenderWidget") { params in
            let messageId = try params.mapOrMissing(for: "messageId") { $0.stringValue }
            let scaleHex = try params.mapOrMissing(for: "scaleHex") { $0.stringValue }
            onRenderWidget(messageId, scaleHex)
            return JSON.dictionaryValue([:])
        }
    }
}

// MARK: - Signing

private extension ContainerBridge {
    func registerSignPayload(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "signPayload") { params in
            let payload = try params.map(
                to: SignTransactionPayload<ProductAccountId>.self
            )

            let result = try await nativeApi.signPayload(payload)
            var response: [String: JSON] = ["signature": JSON.stringValue(result.signature)]
            if let signedTx = result.signedTx {
                response["signedTx"] = JSON.stringValue(signedTx)
            }
            return JSON.dictionaryValue(response)
        }
    }

    func registerSignPayloadLegacy(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "signPayloadLegacy") { params in
            let payload = try params.map(
                to: SignTransactionPayload<SS58Account>.self
            )

            let result = try await nativeApi.signPayloadLegacy(payload)
            var response: [String: JSON] = ["signature": JSON.stringValue(result.signature)]
            if let signedTx = result.signedTx {
                response["signedTx"] = JSON.stringValue(signedTx)
            }
            return JSON.dictionaryValue(response)
        }
    }

    func registerSignRaw(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "signRaw") { params in
            let account = try params.mapOrMissing(for: "account") { try? $0.map(to: ProductAccountId.self) }

            let content: RawPayloadContent
            if let dataHex = params["data"]?.stringValue {
                content = try .bytes(Data(hexString: dataHex))
            } else if let payloadString = params["payload"]?.stringValue {
                content = .payload(payloadString)
            } else {
                throw ContainerBridgeHostApiError.invalidSignRawParams
            }

            let payload = SigningRawPayload(account: account, content: content)
            let result = try await nativeApi.signRaw(payload)

            return JSON.dictionaryValue(["signature": .stringValue(result.signature)])
        }
    }

    func registerSignRawLegacy(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "signRawLegacy") { params in
            let account = try params.mapOrMissing(for: "account") {
                try? $0.map(to: SS58Account.self)
            }.accountId

            let content: RawPayloadContent
            if let dataHex = params["data"]?.stringValue {
                content = try .bytes(Data(hexString: dataHex))
            } else if let payloadString = params["payload"]?.stringValue {
                content = .payload(payloadString)
            } else {
                throw ContainerBridgeHostApiError.invalidSignRawParams
            }

            let payload = SignRawLegacyPayload(account: account, content: content)
            let result = try await nativeApi.signRawLegacy(payload)

            return JSON.dictionaryValue(["signature": .stringValue(result.signature)])
        }
    }

    func registerCreateTransaction(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "createTransaction") { params in
            let payload = try params.map(to: CreateTransactionPayload<ProductAccountId>.self)
            let result = try await nativeApi.createTransaction(payload)
            return JSON.dictionaryValue(["signedTx": .stringValue(result.signedTransaction)])
        }
    }

    func registerCreateTransactionLegacy(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "createTransactionLegacy") { params in
            let payload = try params.map(to: CreateTransactionPayload<LegacyAccountId>.self)
            let result = try await nativeApi.createTransactionLegacy(payload)
            return JSON.dictionaryValue(["signedTx": .stringValue(result.signedTransaction)])
        }
    }
}

// MARK: - Local Storage

private extension ContainerBridge {
    func registerLocalStorageRead(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "localStorageRead") { params in
            let key = try params.mapOrMissing(for: "key") { $0.stringValue }
            let value = try await nativeApi.localStorageRead(key: key)
            return JSON.dictionaryValue(["value": value.map(JSON.stringValue) ?? JSON.null])
        }
    }

    func registerLocalStorageWrite(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "localStorageWrite") { params in
            let key = try params.mapOrMissing(for: "key") { $0.stringValue }
            let value = try params.mapOrMissing(for: "value") { $0.stringValue }
            try await nativeApi.localStorageWrite(key: key, value: value)
            return JSON.dictionaryValue([:])
        }
    }

    func registerLocalStorageClear(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "localStorageClear") { params in
            let key = try params.mapOrMissing(for: "key") { $0.stringValue }
            try await nativeApi.localStorageClear(key: key)
            return JSON.dictionaryValue([:])
        }
    }

    func registerLocalStorageSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "localStorageSubscribe") { params in
            let key = try params.mapOrMissing(for: "key") { $0.stringValue }
            return nativeApi.subscribeLocalStorage(key: key)
                .map { value -> JSON in
                    JSON.dictionaryValue(["value": value.map(JSON.stringValue) ?? JSON.null])
                }
                .eraseToAnyAsyncSequence()
        }
    }
}

// MARK: - Worker Operations

private extension ContainerBridge {
    func registerWorkerBeginOperation(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "workerBeginOperation") { params in
            let label = params["label"]?.stringValue
            let id = try await nativeApi.workerBeginOperation(label: label)
            return JSON.dictionaryValue(["id": .unsignedIntValue(UInt64(id))])
        }
    }

    func registerWorkerEndOperation(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "workerEndOperation") { params in
            let id = try params.mapOrMissing(for: "id") {
                $0.unsignedIntValue.flatMap { UInt32(exactly: $0) }
            }
            try await nativeApi.workerEndOperation(id: id)
            return JSON.dictionaryValue([:])
        }
    }
}

// MARK: - Network Access

private extension ContainerBridge {
    func registerAllowNetworkAccess(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "allowNetworkAccess") { params in
            let url = try params.mapOrMissing(for: "url") { $0.stringValue }
            let allowed = try await nativeApi.allowNetworkAccess(url: url)
            return JSON.dictionaryValue(["allowed": .boolValue(allowed)])
        }
    }

    func registerAllowWebRtcAccess(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "allowWebRtcAccess") { _ in
            let allowed = try await nativeApi.allowWebRtcAccess()
            return JSON.dictionaryValue(["allowed": .boolValue(allowed)])
        }
    }
}

// MARK: - Navigation

private extension ContainerBridge {
    func registerNavigateTo(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "navigateTo") { params in
            let destination = try params.mapOrMissing(for: "destination") { $0.stringValue }
            try await nativeApi.navigateTo(destination: destination)
            return JSON.dictionaryValue([:])
        }
    }
}

// MARK: - Statement Store

private extension ContainerBridge {
    func registerStatementStoreSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "statementStoreSubscribe") { params in
            let dto = try params.map(to: StatementsSubscribeDto.self)

            let source = try nativeApi.subscribeStatements(filter: dto.toStatementStoreFilter())

            return source
                .map { page -> JSON in
                    let statementsJSON = try page.statements.toScaleCompatibleJSON()
                    return .dictionaryValue([
                        "statements": statementsJSON,
                        "isComplete": .boolValue(page.isComplete)
                    ])
                }
                .eraseToAnyAsyncSequence()
        }
    }

    func registerStatementStoreCreateProof(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "createStatementProof") { params in
            let request = try params.map(to: CreateStatementProofDto.self)

            let proof = try await nativeApi.createStatementProof(request)

            return try proof.toScaleCompatibleJSON()
        }
    }

    func registerStatementStoreCreateProofAuthorized(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "createStatementProofAuthorized") { params in
            let request = try params.map(to: CreateStatementProofAuthorizedDto.self)

            let proof = try await nativeApi.createStatementProofAuthorized(request)

            return try proof.toScaleCompatibleJSON()
        }
    }

    func registerStatementStoreSubmit(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "statementStoreSubmit") { params in
            let statement = try params.map(to: StatementDto.self)

            try await nativeApi.submitStatement(statement)

            return JSON.dictionaryValue([:])
        }
    }
}

// MARK: - Payments

private extension ContainerBridge {
    func registerPaymentBalanceSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "paymentBalanceSubscribe") { _ in
            let source = try await nativeApi.subscribePaymentBalance()

            return source
                .map { balance -> JSON in
                    try balance.toScaleCompatibleJSON()
                }
                .eraseToAnyAsyncSequence()
        }
    }

    func registerHostPaymentRequest(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "paymentRequest") { params in
            let request = try params.map(to: PaymentRequestDto.self)

            try await nativeApi.requestPayment(
                amount: request.amount,
                destination: request.destination,
                id: request.id
            )

            return JSON.dictionaryValue([:])
        }
    }

    func registerHostPaymentStatusSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "paymentStatusSubscribe") { params in
            let request = try params.map(to: PaymentStatusSubscribeDto.self)

            return try await nativeApi.subscribePaymentStatus(id: request.id)
                .map { try HostPaymentStatusDto(status: $0).toScaleCompatibleJSON() }
                .eraseToAnyAsyncSequence()
        }
    }

    func registerPaymentTopUp(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "paymentTopUp") { params in
            let request = try params.map(to: PaymentTopUpRequestDto.self)

            try await nativeApi.paymentTopUp(
                amount: request.amount,
                source: request.source,
                id: request.id
            )
            return JSON.dictionaryValue([:])
        }
    }

    func registerPaymentTopUpStatusSubscribe(nativeApi: ProductsNativeApiProtocol) {
        registerSubscriptionHandler(method: "paymentTopUpStatusSubscribe") { params in
            let request = try params.map(to: PaymentTopUpStatusSubscribeDto.self)

            return try await nativeApi.subscribePaymentTopUpStatus(id: request.id)
                .map { try HostPaymentTopUpStatusDto(status: $0).toScaleCompatibleJSON() }
                .eraseToAnyAsyncSequence()
        }
    }
}

// MARK: - Preimage

private extension ContainerBridge {
    func registerPreimageLookup(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "preimageLookup") { params in
            let hash = try params.mapOrMissing(for: "hash") { $0.stringValue }
            let hashData = try Data(hexString: hash)
            let result = try await nativeApi.lookupPreimage(hash: hashData)
            return JSON.dictionaryValue(["data": .stringValue(result.toHex(includePrefix: true))])
        }
    }

    func registerPreimageSubmit(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "preimageSubmit") { params in
            let dataHex = try params.mapOrMissing(for: "data") { $0.stringValue }
            let data = try Data(hexString: dataHex)
            let hash = try await nativeApi.submitPreimage(data: data)
            return JSON.dictionaryValue(["hash": .stringValue(hash)])
        }
    }
}

// MARK: - Permissions

private extension ContainerBridge {
    func registerDevicePermission(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "devicePermission") { params in
            let capability = try params.mapOrMissing(for: "capability") { $0.stringValue }
            let allowed = try await nativeApi.requestDevicePermission(capability: capability)
            return JSON.boolValue(allowed)
        }
    }

    func registerJSDeviceCapability(nativeApi: ProductsNativeApiProtocol) {
        let handler: JSDeviceCapabilityHandler = { capability in
            let granted = try await nativeApi.requestDevicePermission(
                capability: capability.deviceCapabilityType.rawValue
            )
            return granted ? .allowed : .denied
        }
        registerJSDeviceCapabilityHandler(handler)
    }

    func registerRemotePermission(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "remotePermission") { params in
            let requests = try RemotePermissionRequest.fromArray(json: params)
            let allowed = try await nativeApi.requestRemotePermissions(requests)
            return JSON.boolValue(allowed)
        }
    }
}

// MARK: - Push Notification

private extension ContainerBridge {
    func registerPushNotification(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "pushNotification") { params in
            let request = try params.map(to: ScheduledNotificationRequest.self)
            let notificationId = try await nativeApi.pushNotification(request)

            return JSON.dictionaryValue([
                "notificationId": .unsignedIntValue(UInt64(notificationId))
            ])
        }
    }

    func registerCancelPushNotification(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "cancelPushNotification") { params in
            let identifier = try params.mapOrMissing(for: "identifier") {
                $0.unsignedIntValue.flatMap { UInt32(exactly: $0) }
            }
            try await nativeApi.cancelPushNotification(identifier: identifier)
            return JSON.dictionaryValue([:])
        }
    }
}

// MARK: - Entropy Derivation

private extension ContainerBridge {
    func registerDeriveEntropy(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "deriveEntropy") { params in
            let keyHex = try params.mapOrMissing(for: "key") { $0.stringValue }
            let key = try Data(hexString: keyHex)
            let entropy = try await nativeApi.deriveEntropy(key: key)
            return JSON.dictionaryValue([
                "entropy": .stringValue(entropy.toHex(includePrefix: true))
            ])
        }
    }
}

// MARK: - Resource Allocation

private extension ContainerBridge {
    func registerRequestResourceAllocation(nativeApi: ProductsNativeApiProtocol) {
        registerRequestHandler(method: "hostRequestResourceAllocation") { params in
            let request = try params.map(to: RequestResourceAllocationParams.self)

            let outcomes = try await nativeApi.requestResourceAllocation(
                resources: request.resources
            )

            let result = RequestResourceAllocationResult(outcomes: outcomes)
            return try result.toScaleCompatibleJSON()
        }
    }
}

extension JSON {
    func mapOrMissing<T>(for param: String, transform: (JSON) -> T?) throws -> T {
        guard let value = self[param] else {
            throw ContainerBridgeHostApiError.missingRequiredParam(param)
        }

        if let transformed = transform(value) {
            return transformed
        } else {
            throw ContainerBridgeHostApiError.missingRequiredParam(param)
        }
    }
}
