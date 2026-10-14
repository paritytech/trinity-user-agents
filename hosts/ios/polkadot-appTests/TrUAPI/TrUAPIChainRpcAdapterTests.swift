import Foundation
import Testing
import SubstrateSdk
@testable import polkadot_app

struct TrUAPIChainRpcAdapterTests {
    private func makeAdapter() -> (TrUAPIChainRpcAdapter, MockJSONRPCEngine, RecordingAdapterDelegate) {
        let engine = MockJSONRPCEngine()
        let adapter = TrUAPIChainRpcAdapter(engine: engine, logger: StubLogger())
        let delegate = RecordingAdapterDelegate()
        adapter.delegate = delegate
        return (adapter, engine, delegate)
    }

    private func object(_ json: String) -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: json.data(using: .utf8)!)) as? [String: Any] ?? [:]
    }

    private func followEventFrame(subscription: String, event: String) throws -> JSON {
        try JSONDecoder().decode(JSON.self, from: Data(
            #"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"\#(subscription)","result":{"event":"\#(event)"}}}"#
                .utf8
        ))
    }

    private func numericFollowEventFrame(subscription: UInt64, event: String) throws -> JSON {
        try JSONDecoder().decode(JSON.self, from: Data(
            #"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":\#(subscription),"result":{"event":"\#(event)"}}}"#
                .utf8
        ))
    }

    // MARK: plain calls

    @Test func plainCallRoundTripsWithOriginalNumericId() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":42,"method":"state_getStorage","params":["0x00"]}"#)

        let call = try #require(engine.calls.values.first)
        #expect(call.method == "state_getStorage")
        call.completion(.success(.stringValue("0xbeef")))

        let response = try object(#require(delegate.produced.first))
        #expect(response["id"] as? Int == 42)
        #expect(response["result"] as? String == "0xbeef")
    }

    @Test func callErrorSynthesizesErrorEnvelopeWithStringId() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":"a1","method":"m","params":[]}"#)
        try #require(engine.calls.values.first).completion(.failure(JSONRPCEngineError.clientCancelled))

        let response = try object(#require(delegate.produced.first))
        #expect(response["id"] as? String == "a1")
        #expect(response["error"] != nil)
        #expect(response["result"] == nil)
    }

    @Test func rpcErrorCodePassesThrough() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":1,"method":"m","params":[]}"#)
        try #require(engine.calls.values.first).completion(
            .failure(JSONRPCError(message: "bad params", code: -32_602, data: nil))
        )

        let error = try #require(object(delegate.produced[0])["error"] as? [String: Any])
        #expect(error["code"] as? Int == -32_602)
        #expect(error["message"] as? String == "bad params")
    }

    // MARK: subscriptions — real remote id passthrough

    /// The engine reveals the node-assigned id via onSubscribed; the core
    /// sees THAT id, never a synthetic one, before any update frame.
    @Test func subscribeRespondsWithRemoteIdFromOnSubscribed() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)

        let subscription = try #require(engine.subscriptions.values.first)
        #expect(subscription.unsubscribeMethod == "chainHead_v1_unfollow")
        #expect(!subscription.resendOnReconnect)
        #expect(delegate.produced.isEmpty) // response awaits the reveal

        subscription.onSubscribed?(.string("remote-77"))

        let response = object(delegate.produced[0])
        #expect(response["id"] as? Int == 5)
        #expect(response["result"] as? String == "remote-77")

        try subscription.update(followEventFrame(subscription: "remote-77", event: "initialized"))

        let update = object(delegate.produced[1])
        #expect(update["method"] as? String == "chainHead_v1_followEvent")
        let params = try #require(update["params"] as? [String: Any])
        #expect(params["subscription"] as? String == "remote-77")
        #expect((params["result"] as? [String: Any])?["event"] as? String == "initialized")
    }

    /// A numeric node-assigned id round-trips in its original wire type:
    /// numeric in the subscribe response, numeric in forwarded updates, and
    /// matched when the core unsubscribes with the numeric form.
    @Test func numericRemoteIdRoundTripsVerbatim() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        let engineId = try #require(engine.subscriptions.keys.first)
        let subscription = try #require(engine.subscriptions.values.first)

        subscription.onSubscribed?(.number(7))

        let response = object(delegate.produced[0])
        #expect(response["result"] as? UInt64 == 7)

        try subscription.update(numericFollowEventFrame(subscription: 7, event: "initialized"))
        let update = object(delegate.produced[1])
        let params = try #require(update["params"] as? [String: Any])
        #expect(params["subscription"] as? UInt64 == 7)

        adapter.handle(request: #"{"jsonrpc":"2.0","id":6,"method":"chainHead_v1_unfollow","params":[7]}"#)

        #expect(engine.cancelled == [engineId])
        let unfollowResponse = try object(#require(delegate.produced.last))
        #expect(unfollowResponse["id"] as? Int == 6)
        #expect(unfollowResponse["result"] is NSNull)
    }

    /// Regression for create_proof's BlockHeaderNotFound: chainHead
    /// operations reference the follow id inside plain-call params; with the
    /// real remote id passed through, the operation call reaches the engine
    /// with the id the node actually knows.
    @Test func chainHeadOperationParamsPassThroughUnchanged() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        try #require(engine.subscriptions.values.first).onSubscribed?(.string("remote-77"))
        let followId = try #require(object(delegate.produced[0])["result"] as? String)

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":6,"method":"chainHead_v1_header","params":["\#(followId)","0xabcd"]}"#
        )

        let call = try #require(engine.calls.values.first)
        #expect(call.method == "chainHead_v1_header")
        #expect(call.params?.arrayValue?.first?.stringValue == "remote-77")
        #expect(call.params?.arrayValue?.last?.stringValue == "0xabcd")
    }

    @Test func statefulFailureSynthesizesStopEventWithRemoteId() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        let subscription = try #require(engine.subscriptions.values.first)
        subscription.onSubscribed?(.string("remote-77"))

        subscription.failure(JSONRPCEngineError.clientCancelled, false)

        let update = try object(#require(delegate.produced.last))
        #expect(update["method"] as? String == "chainHead_v1_followEvent")
        let params = try #require(update["params"] as? [String: Any])
        #expect(params["subscription"] as? String == "remote-77")
        #expect((params["result"] as? [String: Any])?["event"] as? String == "stop")
    }

    /// Death before the reveal: the core still awaits its subscribe
    /// response — it must get an error envelope, not silence.
    @Test func subscribeFailureBeforeRevealSynthesizesErrorResponse() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        try #require(engine.subscriptions.values.first)
            .failure(JSONRPCEngineError.clientCancelled, false)

        let response = try object(#require(delegate.produced.first))
        #expect(response["id"] as? Int == 5)
        #expect(response["error"] != nil)
    }

    // MARK: unsubscribe result shapes

    @Test func unfollowCancelsAndAnswersNull() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        let engineId = try #require(engine.subscriptions.keys.first)
        try #require(engine.subscriptions.values.first).onSubscribed?(.string("remote-77"))

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":6,"method":"chainHead_v1_unfollow","params":["remote-77"]}"#
        )

        #expect(engine.cancelled == [engineId])
        let response = try object(#require(delegate.produced.last))
        #expect(response["id"] as? Int == 6)
        #expect(response["result"] is NSNull)
        #expect(response["error"] == nil)
    }

    @Test func unknownUnfollowAnswersInvalidSubscriptionError() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":6,"method":"chainHead_v1_unfollow","params":["nope"]}"#
        )

        #expect(engine.cancelled.isEmpty)
        let response = object(delegate.produced[0])
        let error = try #require(response["error"] as? [String: Any])
        #expect(error["code"] as? Int == -32_602)
    }

    @Test func legacyUnsubscribeAnswersBool() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":2,"method":"state_subscribeStorage","params":[]}"#)
        let engineId = try #require(engine.subscriptions.keys.first)
        try #require(engine.subscriptions.values.first).onSubscribed?(.string("sub-1"))

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":3,"method":"state_unsubscribeStorage","params":["sub-1"]}"#
        )
        #expect(engine.cancelled == [engineId])
        #expect(try object(#require(delegate.produced.last))["result"] as? Bool == true)

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":4,"method":"state_unsubscribeStorage","params":["sub-1"]}"#
        )
        #expect(try object(#require(delegate.produced.last))["result"] as? Bool == false)
    }

    @Test func statementStoreSubscriptionForwardsPages() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":8,"method":"statement_subscribeStatement","params":[{"matchAny":["0x11"]}]}"#
        )

        let subscription = try #require(engine.subscriptions.values.first)
        #expect(subscription.unsubscribeMethod == "statement_unsubscribeStatement")
        #expect(delegate.produced.isEmpty)

        subscription.onSubscribed?(.string("stmt-1"))
        #expect(object(delegate.produced[0])["result"] as? String == "stmt-1")

        try subscription.update(JSONDecoder().decode(JSON.self, from: Data(
            #"{"jsonrpc":"2.0","method":"statement_subscribeStatement","params":{"subscription":"stmt-1","result":{"event":"newStatements","data":{"statements":["0x00"],"remaining":0}}}}"#
                .utf8
        )))

        let update = object(delegate.produced[1])
        let params = try #require(update["params"] as? [String: Any])
        #expect(params["subscription"] as? String == "stmt-1")
        let result = try #require(params["result"] as? [String: Any])
        #expect(result["event"] as? String == "newStatements")
        #expect(engine.cancelled.isEmpty)

        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":9,"method":"statement_unsubscribeStatement","params":["stmt-1"]}"#
        )
        #expect(try object(#require(delegate.produced.last))["result"] as? Bool == true)
    }

    // MARK: terminal events

    /// A node-sent terminal event is forwarded, then the engine subscription
    /// is released silently (no wire unsubscribe — the stream is dead).
    @Test func terminalEventReleasesEngineSubscriptionSilently() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        let engineId = try #require(engine.subscriptions.keys.first)
        let subscription = try #require(engine.subscriptions.values.first)
        subscription.onSubscribed?(.string("remote-77"))

        try subscription.update(followEventFrame(subscription: "remote-77", event: "stop"))

        let forwarded = try object(#require(delegate.produced.last))
        let params = try #require(forwarded["params"] as? [String: Any])
        #expect((params["result"] as? [String: Any])?["event"] as? String == "stop")

        #expect(engine.silentlyCancelled == [engineId])
        #expect(engine.cancelled.isEmpty)

        // The optional post-stop unfollow now targets an unknown id.
        adapter.handle(
            request: #"{"jsonrpc":"2.0","id":6,"method":"chainHead_v1_unfollow","params":["remote-77"]}"#
        )
        let response = try object(#require(delegate.produced.last))
        #expect(response["error"] != nil)
    }

    @Test func operationEventDoesNotReleaseSubscription() throws {
        let (adapter, engine, delegate) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":5,"method":"chainHead_v1_follow","params":[true]}"#)
        let subscription = try #require(engine.subscriptions.values.first)
        subscription.onSubscribed?(.string("remote-77"))

        try subscription.update(
            followEventFrame(subscription: "remote-77", event: "operationBodyDone")
        )

        #expect(engine.silentlyCancelled.isEmpty)
        #expect(delegate.produced.count == 2) // reveal + forwarded frame
    }

    // MARK: teardown / unsupported

    @Test func tearDownCancelsEverything() {
        let (adapter, engine, _) = makeAdapter()

        adapter.handle(request: #"{"jsonrpc":"2.0","id":1,"method":"m","params":[]}"#)
        adapter.handle(request: #"{"jsonrpc":"2.0","id":2,"method":"chain_subscribeNewHeads","params":[]}"#)

        adapter.tearDown()

        #expect(Set(engine.cancelled) == Set(engine.calls.keys).union(engine.subscriptions.keys))
    }

    @Test func unsupportedFrameProducesNothingAndDoesNotCrash() {
        let (adapter, _, delegate) = makeAdapter()
        adapter.handle(request: #"[{"id":1}]"#)
        adapter.handle(request: "garbage")
        #expect(delegate.produced.isEmpty)
    }
}
