import Foundation
import os
import SDKLogger
import SubstrateSdk

public protocol TrUAPIChainRpcAdapterDelegate: AnyObject {
    /// A synthesized response, a verbatim notification frame, or a
    /// synthesized termination notification for the core.
    func adapter(_ adapter: TrUAPIChainRpcAdapter, didProduce json: String)
}

/// Verbatim JSON-RPC pipe adapter for ONE logical chain connection: parses
/// core frames and drives the shared typed engine. The engine owns wire ids,
/// queueing, reconnection, and remote-call routing; the adapter synthesizes
/// response envelopes with the core's original request ids.
///
/// Subscription ids are NOT masked: the node-assigned id (revealed through
/// the engine's `onSubscribed`) answers the core's subscribe request in its
/// original wire type — chainHead operations reference it inside later call
/// params, so a made-up or retyped id would strand every operation (unknown
/// subscription on the node). Consequently the engine never resubscribes:
/// stateful families surface engine reconnects as their protocol's
/// termination event; legacy families have no such signal and their streams
/// silently end (warned at subscribe). Node-sent terminal events release the
/// engine subscription without a wire unsubscribe — the stream is already
/// dead on the node.
public final class TrUAPIChainRpcAdapter: @unchecked Sendable {
    /// Mutable bookkeeping for one live subscription; guarded by `state`.
    fileprivate final class SubscriptionState {
        let family: TrUAPISubscriptionMethods.Family
        var engineId: UInt16?
        var pendingRequestId: TrUAPIRpcId?
        var remoteId: TrUAPISubscriptionId?

        init(family: TrUAPISubscriptionMethods.Family, pendingRequestId: TrUAPIRpcId) {
            self.family = family
            self.pendingRequestId = pendingRequestId
        }
    }

    fileprivate struct State {
        var inFlightCalls: Set<UInt16> = []
        var subscriptions: [ObjectIdentifier: SubscriptionState] = [:]
        var subscriptionsByRemoteId: [TrUAPISubscriptionId: SubscriptionState] = [:]
    }

    public weak var delegate: TrUAPIChainRpcAdapterDelegate?

    private let engine: JSONRPCEngine
    private let logger: SDKLoggerProtocol
    private let state = OSAllocatedUnfairLock<State>(initialState: State())
    private let jsonEncoder = JSONEncoder()

    public init(engine: JSONRPCEngine, logger: SDKLoggerProtocol) {
        self.engine = engine
        self.logger = logger
    }

    /// Handle one raw outgoing frame from the core.
    public func handle(request: String) {
        switch TrUAPIRpcFrame.parse(request) {
        case let .request(id, method, params):
            route(id: id, method: method, params: params)
        case let .notification(method, _):
            logger.warning("TrUAPI rpc adapter: dropping core notification \(method)")
        case .unsupported:
            logger.error("TrUAPI rpc adapter: unsupported frame dropped")
        }
    }

    /// Cancel all in-flight calls and unsubscribe all live subscriptions.
    public func tearDown() {
        let ids = state.withLock { state -> [UInt16] in
            let ids = Array(state.inFlightCalls) + state.subscriptions.values.compactMap(\.engineId)
            state.inFlightCalls.removeAll()
            state.subscriptions.removeAll()
            state.subscriptionsByRemoteId.removeAll()
            return ids
        }

        guard !ids.isEmpty else { return }
        engine.cancelForIdentifiers(ids)
    }
}

// MARK: - Routing

private extension TrUAPIChainRpcAdapter {
    func route(id: TrUAPIRpcId, method: String, params: JSON?) {
        if let family = TrUAPISubscriptionMethods.family(forSubscribe: method) {
            subscribe(id: id, method: method, params: params, family: family)
        } else if let family = TrUAPISubscriptionMethods.family(forUnsubscribe: method) {
            unsubscribe(id: id, params: params, family: family)
        } else {
            call(id: id, method: method, params: params)
        }
    }

    func call(id: TrUAPIRpcId, method: String, params: JSON?) {
        do {
            logger.debug("calling [\(id)] \(method) with params: \(String(describing: params))")

            // Written once before the completion can observe it; the engine
            // invokes the completion only after callMethod returns.
            nonisolated(unsafe) var engineId: UInt16 = 0
            engineId = try engine.callMethod(
                method,
                params: params,
                options: JSONRPCOptions(resendOnReconnect: true)
            ) { [weak self, logger] (result: Result<JSON, Error>) in
                logger.debug("call [\(id)] completed \(result)")
                self?.finishCall(engineId: engineId, originalId: id, result: result)
            }

            state.withLock { _ = $0.inFlightCalls.insert(engineId) }
        } catch {
            emitError(id: id, error: error)
        }
    }

    func subscribe(
        id: TrUAPIRpcId,
        method: String,
        params: JSON?,
        family: TrUAPISubscriptionMethods.Family
    ) {
        if family.events == nil {
            logger.warning(
                "TrUAPI rpc adapter: legacy subscription \(method) will not survive reconnects"
            )
        }

        let subscription = SubscriptionState(family: family, pendingRequestId: id)

        do {
            logger.debug("subscribing [\(id)] \(method) with params: \(String(describing: params))")

            let engineId = try engine.subscribe(
                method,
                params: params,
                unsubscribeMethod: family.unsubscribeMethod,
                options: JSONRPCOptions(resendOnReconnect: false),
                onSubscribed: { [weak self] remoteId in
                    self?.confirmSubscription(subscription, remoteId: remoteId)
                },
                updateClosure: { [weak self] (frame: JSON) in
                    self?.deliverUpdate(subscription: subscription, frame: frame)
                },
                failureClosure: { [weak self, logger] error, _ in
                    logger.debug("subscription failed [\(id)] \(method) with error: \(error)")
                    self?.finishSubscription(subscription: subscription, error: error)
                }
            )

            state.withLock { state in
                subscription.engineId = engineId
                state.subscriptions[ObjectIdentifier(subscription)] = subscription
            }
        } catch {
            emitError(id: id, error: error)
        }
    }

    func unsubscribe(id: TrUAPIRpcId, params: JSON?, family: TrUAPISubscriptionMethods.Family) {
        logger.debug("unsubscribe [\(id)] with params: \(String(describing: params))")

        let remoteId = params?.arrayValue?.first.flatMap(TrUAPISubscriptionId.init(paramValue:))

        let engineId = state.withLock { state -> UInt16? in
            guard let remoteId, let found = state.subscriptionsByRemoteId[remoteId] else {
                return nil
            }
            state.drop(subscription: found)
            return found.engineId
        }

        if let engineId {
            engine.cancelForIdentifiers([engineId])
        }

        switch (engineId != nil, family.unsubscribeResult) {
        case (true, .null):
            emitResponse(id: id, result: JSON.null)
        case (true, .bool):
            emitResponse(id: id, result: true)
        case (false, .null):
            emitError(
                id: id,
                error: JSONRPCError(message: "Invalid subscription", code: -32_602, data: nil)
            )
        case (false, .bool):
            emitResponse(id: id, result: false)
        }
    }
}

// MARK: - Engine callbacks

private extension TrUAPIChainRpcAdapter {
    func finishCall(engineId: UInt16, originalId: TrUAPIRpcId, result: Result<JSON, Error>) {
        state.withLock { _ = $0.inFlightCalls.remove(engineId) }

        switch result {
        case let .success(json):
            emitResponse(id: originalId, result: json)
        case let .failure(error):
            emitError(id: originalId, error: error)
        }
    }

    /// Answer the core's subscribe request with the node-assigned id, in its
    /// original wire type, as soon as the engine reveals it.
    func confirmSubscription(_ subscription: SubscriptionState, remoteId: TrUAPISubscriptionId) {
        let requestId = state.withLock { state -> TrUAPIRpcId? in
            guard state.subscriptions[ObjectIdentifier(subscription)] != nil else {
                return nil
            }

            subscription.remoteId = remoteId
            state.subscriptionsByRemoteId[remoteId] = subscription

            defer { subscription.pendingRequestId = nil }
            return subscription.pendingRequestId
        }

        guard let requestId else { return }
        emitResponse(id: requestId, result: remoteId)
    }

    /// Forward the frame verbatim; a terminal event additionally releases
    /// the subscription engine-side.
    func deliverUpdate(subscription: SubscriptionState, frame: JSON) {
        let alive = state.withLock { $0.subscriptions[ObjectIdentifier(subscription)] != nil }
        guard alive else { return }

        let data: Data
        do {
            data = try jsonEncoder.encode(frame)
        } catch {
            logger.error("TrUAPI rpc adapter: update forwarding failed: \(error)")
            return
        }

        guard let json = String(data: data, encoding: .utf8) else { return }
        delegate?.adapter(self, didProduce: json)

        if let event = terminalEvent(in: data, for: subscription.family) {
            releaseTerminated(subscription: subscription, event: event)
        }
    }

    func terminalEvent(in data: Data, for family: TrUAPISubscriptionMethods.Family) -> String? {
        guard
            let events = family.events,
            let update = try? JSONDecoder().decode(TrUAPIRpcUpdateFrame.self, from: data),
            update.method == events.notificationMethod,
            let event = update.params.result?.dictValue?["event"]?.stringValue,
            events.terminalEvents.contains(event)
        else {
            return nil
        }
        return event
    }

    func releaseTerminated(subscription: SubscriptionState, event: String) {
        let engineId = state.withLock { state -> UInt16? in
            guard state.subscriptions[ObjectIdentifier(subscription)] != nil else {
                return nil
            }
            state.drop(subscription: subscription)
            return subscription.engineId
        }

        guard let engineId else { return }
        engine.cancelForIdentifiers([engineId], sendUnsubscribe: false)
        logger.debug("TrUAPI rpc adapter: subscription released by terminal event \(event)")
    }

    func finishSubscription(subscription: SubscriptionState, error: Error) {
        let alive = state.withLock { state -> Bool in
            guard state.subscriptions[ObjectIdentifier(subscription)] != nil else {
                return false
            }
            state.drop(subscription: subscription)
            return true
        }

        guard alive else { return }

        if let pendingRequestId = subscription.pendingRequestId {
            // Died before the reveal — the core still awaits its subscribe
            // response.
            emitError(id: pendingRequestId, error: error)
        } else if let events = subscription.family.events, let remoteId = subscription.remoteId {
            emitNotification(
                method: events.notificationMethod,
                subscription: remoteId,
                result: events.connectionLossEvent
            )
        } else {
            logger.debug("TrUAPI rpc adapter: subscription ended: \(error)")
        }
    }
}

private extension TrUAPIChainRpcAdapter.State {
    mutating func drop(subscription: TrUAPIChainRpcAdapter.SubscriptionState) {
        subscriptions[ObjectIdentifier(subscription)] = nil
        if let remoteId = subscription.remoteId {
            subscriptionsByRemoteId[remoteId] = nil
        }
    }
}

// MARK: - Envelope synthesis

private extension TrUAPIChainRpcAdapter {
    func emitResponse(id: TrUAPIRpcId, result: some Encodable) {
        emit(TrUAPIRpcResponse(id: id, result: result))
    }

    func emitError(id: TrUAPIRpcId, error: Error) {
        let rpcError = error as? JSONRPCError
            ?? JSONRPCError(message: "\(error)", code: -32_603, data: nil)
        emit(TrUAPIRpcErrorResponse(id: id, error: rpcError))
    }

    func emitNotification(method: String, subscription: TrUAPISubscriptionId, result: JSON) {
        emit(TrUAPIRpcNotification(
            method: method,
            params: TrUAPIRpcNotification.Params(subscription: subscription, result: result)
        ))
    }

    func emit(_ envelope: some Encodable) {
        do {
            let data = try jsonEncoder.encode(envelope)
            guard let json = String(data: data, encoding: .utf8) else { return }
            delegate?.adapter(self, didProduce: json)
        } catch {
            logger.error("TrUAPI rpc adapter: envelope encoding failed: \(error)")
        }
    }
}
