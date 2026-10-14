import Foundation
import Testing
import SubstrateSdk
@testable import polkadot_app

struct TrUAPIRpcFrameTests {
    @Test func parsesRequestWithNumericIdAndArrayParams() {
        let frame = TrUAPIRpcFrame.parse(
            #"{"jsonrpc":"2.0","id":7,"method":"chainHead_v1_follow","params":[true]}"#
        )
        guard case let .request(id, method, params) = frame else {
            Issue.record("expected request, got \(frame)")
            return
        }
        #expect(id == .number(7))
        #expect(method == "chainHead_v1_follow")
        #expect(params?.arrayValue?.first?.boolValue == true)
    }

    @Test func parsesRequestWithStringIdAndNoParams() {
        let frame = TrUAPIRpcFrame.parse(#"{"jsonrpc":"2.0","id":"a-1","method":"system_health"}"#)
        guard case let .request(id, _, params) = frame else {
            Issue.record("expected request, got \(frame)")
            return
        }
        #expect(id == .string("a-1"))
        #expect(params == nil)
    }

    @Test func parsesNotification() {
        let frame = TrUAPIRpcFrame.parse(#"{"jsonrpc":"2.0","method":"m","params":[]}"#)
        guard case .notification("m", _) = frame else {
            Issue.record("expected notification, got \(frame)")
            return
        }
    }

    @Test func batchResponseAndGarbageAreUnsupported() {
        for raw in [#"[{"id":1},{"id":2}]"#, #"{"jsonrpc":"2.0","id":3,"result":1}"#, "not json"] {
            guard case .unsupported = TrUAPIRpcFrame.parse(raw) else {
                Issue.record("expected unsupported for \(raw)")
                return
            }
        }
    }

    @Test func chainHeadFamilyIsStateful() throws {
        let family = try #require(TrUAPISubscriptionMethods.family(forSubscribe: "chainHead_v1_follow"))
        #expect(family.unsubscribeMethod == "chainHead_v1_unfollow")
        #expect(family.unsubscribeResult == .null)

        let events = try #require(family.events)
        #expect(events.notificationMethod == "chainHead_v1_followEvent")
        #expect(events.terminalEvents == ["stop"])
        #expect(events.connectionLossEvent.dictValue?["event"]?.stringValue == "stop")
    }

    @Test func transactionWatchFamilyUsesSpecUnwatchMethod() throws {
        let family = try #require(
            TrUAPISubscriptionMethods.family(forSubscribe: "transactionWatch_v1_submitAndWatch")
        )
        #expect(family.unsubscribeMethod == "transactionWatch_v1_unwatch")
        #expect(family.unsubscribeResult == .null)

        let events = try #require(family.events)
        #expect(events.notificationMethod == "transactionWatch_v1_watchEvent")
        #expect(events.terminalEvents == ["finalized", "error", "invalid", "dropped"])
        #expect(events.connectionLossEvent.dictValue?["event"]?.stringValue == "dropped")
    }

    @Test(arguments: [
        ("archive_v1_storage", "archive_v1_stopStorage", "archive_v1_storageEvent", "storageError"),
        ("archive_v1_storageDiff", "archive_v1_stopStorageDiff", "archive_v1_storageDiffEvent", "storageDiffError")
    ])
    func archiveFamiliesTerminateWithErrorEvent(
        subscribe: String,
        unsubscribe: String,
        notification: String,
        errorEvent: String
    ) throws {
        let family = try #require(TrUAPISubscriptionMethods.family(forSubscribe: subscribe))
        #expect(family.unsubscribeMethod == unsubscribe)
        #expect(family.unsubscribeResult == .null)

        let events = try #require(family.events)
        #expect(events.notificationMethod == notification)
        #expect(events.terminalEvents.contains(errorEvent))
        #expect(events.connectionLossEvent.dictValue?["event"]?.stringValue == errorEvent)
    }

    @Test(arguments: [
        ("chain_subscribeNewHeads", "chain_unsubscribeNewHeads"),
        ("chain_subscribeNewHead", "chain_unsubscribeNewHead"),
        ("subscribe_newHead", "unsubscribe_newHead"),
        ("chain_subscribeAllHeads", "chain_unsubscribeAllHeads"),
        ("chain_subscribeFinalizedHeads", "chain_unsubscribeFinalizedHeads"),
        ("chain_subscribeFinalisedHeads", "chain_unsubscribeFinalisedHeads"),
        ("state_subscribeStorage", "state_unsubscribeStorage"),
        ("state_subscribeRuntimeVersion", "state_unsubscribeRuntimeVersion"),
        ("chain_subscribeRuntimeVersion", "chain_unsubscribeRuntimeVersion"),
        ("grandpa_subscribeJustifications", "grandpa_unsubscribeJustifications"),
        ("beefy_subscribeJustifications", "beefy_unsubscribeJustifications"),
        ("author_submitAndWatchExtrinsic", "author_unwatchExtrinsic")
    ])
    func legacyFamilyIsExplicitAndStateless(subscribe: String, unsubscribe: String) throws {
        let family = try #require(TrUAPISubscriptionMethods.family(forSubscribe: subscribe))
        #expect(family.unsubscribeMethod == unsubscribe)
        #expect(family.unsubscribeResult == .bool)
        #expect(family.events == nil)
    }

    @Test func unsubscribeDetection() {
        #expect(TrUAPISubscriptionMethods.isUnsubscribe("chainHead_v1_unfollow"))
        #expect(TrUAPISubscriptionMethods.isUnsubscribe("chain_unsubscribeNewHeads"))
        #expect(TrUAPISubscriptionMethods.isUnsubscribe("author_unwatchExtrinsic"))
        #expect(!TrUAPISubscriptionMethods.isUnsubscribe("state_getStorage"))
        #expect(
            TrUAPISubscriptionMethods.family(forUnsubscribe: "chainHead_v1_unfollow")?
                .unsubscribeResult == .null
        )
    }

    @Test func methodsOutsideTableAreNotClassified() {
        #expect(TrUAPISubscriptionMethods.family(forSubscribe: "foo_subscribeBar") == nil)
        #expect(!TrUAPISubscriptionMethods.isUnsubscribe("foo_unsubscribeBar"))
    }
}
