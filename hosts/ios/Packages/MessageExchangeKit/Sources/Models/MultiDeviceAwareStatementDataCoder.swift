import Foundation
import SubstrateSdk
import SDKLogger

/// A `StatementDataCoding` implementation for multi-device messaging.
///
/// - Encoding: wraps `Request`/`Response` in `MultiDeviceRequest`/`MultiDeviceResponse`
///   (inner one-shot ChaCha20-Poly1305 key per device), then applies outer X25519 encryption via `outgoingCoder`
///   = ECDH(own_device_priv, peer_identity_pub).
///
/// - Decoding: selects the outer coder based on sender account ID:
///   - own account → `outgoingCoder` (own statements were encoded with the same key)
///   - known peer device account → per-device coder ECDH(own_identity_priv, peer_device_pub)
///   - unknown sender → throws `deviceEntryNotFound`
///   After outer decryption, inner decryption is performed for `.multirequest`/`.multiresponse`.
public final class MultiDeviceAwareStatementDataCoder: StatementDataCoding {
    private let outgoingCoder: StatementDataCoding
    private let identityCoder: StatementDataCoding
    private let incomingCoders: [AccountId: StatementDataCoding]
    private let multiDeviceCoder: MultiDeviceStatementDataCoding
    private let recipientDevices: [RecipientDeviceInfo]
    private let deviceKeysByAccountId: [AccountId: Data]
    private let ownStatementAccountId: AccountId
    private let logger: SDKLoggerProtocol?

    public init(
        outgoingCoder: StatementDataCoding,
        identityCoder: StatementDataCoding,
        incomingCoders: [AccountId: StatementDataCoding],
        multiDeviceCoder: MultiDeviceStatementDataCoding,
        recipientDevices: [RecipientDeviceInfo],
        ownStatementAccountId: AccountId,
        deviceKeysByAccountId: [AccountId: Data],
        logger: SDKLoggerProtocol?
    ) {
        self.outgoingCoder = outgoingCoder
        self.identityCoder = identityCoder
        self.incomingCoders = incomingCoders
        self.multiDeviceCoder = multiDeviceCoder
        self.recipientDevices = recipientDevices
        self.ownStatementAccountId = ownStatementAccountId
        self.deviceKeysByAccountId = deviceKeysByAccountId
        self.logger = logger
    }

    public func encodeToScaleEncodedPayload(
        _ statementData: StatementData<some MessageExchange.CodableMessage>,
        route: PeerSessionRoute
    ) throws -> Data {
        switch route {
        case .device:
            try encodeToScaleEncodedPayload(statementData)
        case .identity:
            try identityCoder.encodeToScaleEncodedPayload(statementData, route: route)
        }
    }

    public func decodeFromScaleEncodedPayload<M: MessageExchange.CodableMessage>(
        _ payload: Data,
        senderAccountId: Data?,
        route: PeerSessionRoute
    ) throws -> StatementDataDecodingResult<M> {
        switch route {
        case .device:
            try decodeFromScaleEncodedPayload(payload, senderAccountId: senderAccountId)
        case .identity:
            try identityCoder.decodeFromScaleEncodedPayload(
                payload,
                senderAccountId: senderAccountId,
                route: route
            )
        }
    }
}

// MARK: - Private

private extension MultiDeviceAwareStatementDataCoder {
    func encodeToScaleEncodedPayload<M: MessageExchange.CodableMessage>(
        _ statementData: StatementData<M>
    ) throws -> Data {
        switch statementData {
        case let .request(request):
            try encodeRequest(request)
        case let .response(response):
            try encodeResponse(response, as: M.self)
        case .multirequest,
             .multiresponse:
            try outgoingCoder.encodeToScaleEncodedPayload(statementData, route: .device)
        }
    }

    func encodeRequest<M: MessageExchange.CodableMessage>(_ request: MessageExchange.Request<M>) throws -> Data {
        let multiRequest = try multiDeviceCoder.encodeMultiDeviceRequest(request, recipients: recipientDevices)
        return try outgoingCoder.encodeToScaleEncodedPayload(
            StatementData<M>.multirequest(multiRequest),
            route: .device
        )
    }

    func encodeResponse<M: MessageExchange.CodableMessage>(
        _ response: MessageExchange.Response,
        as _: M.Type
    ) throws -> Data {
        let multiResponse = try multiDeviceCoder.encodeMultiDeviceResponse(response, recipients: recipientDevices)
        return try outgoingCoder.encodeToScaleEncodedPayload(
            StatementData<M>.multiresponse(multiResponse),
            route: .device
        )
    }

    func decodeFromScaleEncodedPayload<M: MessageExchange.CodableMessage>(
        _ payload: Data,
        senderAccountId: Data?
    ) throws -> StatementDataDecodingResult<M> {
        let outerCoder = try selectOuterCoder(for: senderAccountId)
        let result: StatementDataDecodingResult<M> = try outerCoder.decodeFromScaleEncodedPayload(
            payload,
            senderAccountId: senderAccountId,
            route: .device
        )
        return try unwrapMultiDeviceResult(result, senderAccountId: senderAccountId)
    }

    func selectOuterCoder(for senderAccountId: Data?) throws -> StatementDataCoding {
        if let senderAccountId, let deviceCoder = incomingCoders[senderAccountId] {
            return deviceCoder
        } else if senderAccountId == ownStatementAccountId {
            return outgoingCoder
        } else {
            logger?.error("Unknown sender=\(senderAccountId?.toHex() ?? "nil")")
            throw MultiDeviceDecodingError.deviceEntryNotFound
        }
    }

    func unwrapMultiDeviceResult<M: MessageExchange.CodableMessage>(
        _ result: StatementDataDecodingResult<M>,
        senderAccountId: Data?
    ) throws -> StatementDataDecodingResult<M> {
        switch result {
        case .requestId:
            result
        case let .statementData(.multirequest(multiRequest)):
            try decodeMultirequest(multiRequest, senderAccountId: senderAccountId)
        case let .statementData(.multiresponse(multiResponse)):
            try decodeMultiresponse(multiResponse, senderAccountId: senderAccountId)
        case .statementData(.request),
             .statementData(.response):
            result
        }
    }

    func decodeMultirequest<M: MessageExchange.CodableMessage>(
        _ multiRequest: MultiDeviceRequest,
        senderAccountId: Data?
    ) throws -> StatementDataDecodingResult<M> {
        let (entryAccountId, devicePublicKey) = try resolveDecryptionContext(
            senderAccountId: senderAccountId,
            devicesInfo: multiRequest.devicesInfo
        )
        let request: MessageExchange.Request<M> = try multiDeviceCoder.decodeMultiDeviceRequest(
            multiRequest,
            ownStatementAccountId: entryAccountId,
            peerDevicePublicKey: devicePublicKey
        )
        return .statementData(.request(request))
    }

    func decodeMultiresponse<M: MessageExchange.CodableMessage>(
        _ multiResponse: MultiDeviceResponse,
        senderAccountId: Data?
    ) throws -> StatementDataDecodingResult<M> {
        let (entryAccountId, devicePublicKey) = try resolveDecryptionContext(
            senderAccountId: senderAccountId,
            devicesInfo: multiResponse.devicesInfo
        )
        let response = try multiDeviceCoder.decodeMultiDeviceResponse(
            multiResponse,
            ownStatementAccountId: entryAccountId,
            peerDevicePublicKey: devicePublicKey
        )
        return .statementData(.response(response))
    }

    func resolveDecryptionContext(
        senderAccountId: Data?,
        devicesInfo: [RequestDeviceInfo]
    ) throws -> (entryAccountId: AccountId, devicePublicKey: Data) {
        if senderAccountId == ownStatementAccountId {
            try resolveOwnStatementDecryptionContext(devicesInfo: devicesInfo)
        } else {
            try resolvePeerStatementDecryptionContext(senderAccountId: senderAccountId)
        }
    }

    // We are the sender — use any peer entry from devicesInfo. Valid because we encrypted
    // each peer entry with ECDH(our_private, peer_public), so decryption uses the same derivation.
    func resolveOwnStatementDecryptionContext(
        devicesInfo: [RequestDeviceInfo]
    ) throws -> (entryAccountId: AccountId, devicePublicKey: Data) {
        guard
            let entry = devicesInfo.first(where: { deviceKeysByAccountId[$0.statementAccountId] != nil }),
            let publicKey = deviceKeysByAccountId[entry.statementAccountId]
        else {
            logger?.error("Own statement has no known peer device entry")
            throw MultiDeviceDecodingError.deviceEntryNotFound
        }
        return (entry.statementAccountId, publicKey)
    }

    // Peer is the sender — look up their public key and use our own entry in devicesInfo.
    func resolvePeerStatementDecryptionContext(
        senderAccountId: Data?
    ) throws -> (entryAccountId: AccountId, devicePublicKey: Data) {
        guard let accountId = senderAccountId else {
            logger?.error("No sender accountId")
            throw MultiDeviceDecodingError.deviceEntryNotFound
        }
        guard let publicKey = deviceKeysByAccountId[accountId] else {
            logger?.error("Sender \(accountId.toHex()) not in deviceKeysByAccountId")
            throw MultiDeviceDecodingError.deviceEntryNotFound
        }
        return (ownStatementAccountId, publicKey)
    }
}
