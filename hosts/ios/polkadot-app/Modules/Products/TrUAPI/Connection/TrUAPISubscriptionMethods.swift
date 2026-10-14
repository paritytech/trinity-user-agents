import Foundation
import SubstrateSdk

/// Substrate subscription families. The adapter never resubscribes: the
/// node-assigned subscription id is handed to the core verbatim (chainHead
/// operations reference it inside later call params), so a silent
/// resubscribe would strand the core on a dead id. Stateful spec-v2
/// families surface engine reconnects as their protocol's termination
/// event; legacy families have no such signal — their streams silently end
/// on reconnect, which is why subscribing to one logs a warning.
public enum TrUAPISubscriptionMethods {
    /// Wire shape of a successful unsubscribe response: spec-v2 methods
    /// return null (unknown ids get a JSON-RPC error), legacy pubsub
    /// returns a bool.
    public enum UnsubscribeResult {
        case null
        case bool
    }

    /// Event protocol of a stateful family: the notification method carrying
    /// its events, the event values that end the subscription, and the event
    /// synthesized when the engine loses the connection.
    public struct Events {
        public let notificationMethod: String
        public let terminalEvents: Set<String>
        public let connectionLossEvent: JSON
    }

    public struct Family {
        public let unsubscribeMethod: String
        public let unsubscribeResult: UnsubscribeResult
        /// nil → legacy family: no event protocol exists.
        public let events: Events?
    }

    /// Closed table of subscription families the adapter recognizes; any
    /// method outside it routes as a plain call, so a new family used by a
    /// future core must be added here or its updates are never forwarded.
    ///
    /// chainHead_v1_follow is the only subscription today's core opens
    /// (transactions go through plain transaction_v1_broadcast calls); the
    /// other entries are defensive for future core versions. Per the spec,
    /// `dropped` is the terminal "server lost tracking, transaction outcome
    /// unknown" event — the truthful description of upstream connection
    /// loss; `error`/`invalid` would wrongly assert a verdict.
    private static let families: [String: Family] = [
        "chainHead_v1_follow": Family(
            unsubscribeMethod: "chainHead_v1_unfollow",
            unsubscribeResult: .null,
            events: Events(
                notificationMethod: "chainHead_v1_followEvent",
                terminalEvents: ["stop"],
                connectionLossEvent: .dictionaryValue(["event": .stringValue("stop")])
            )
        ),
        "transactionWatch_v1_submitAndWatch": Family(
            unsubscribeMethod: "transactionWatch_v1_unwatch",
            unsubscribeResult: .null,
            events: Events(
                notificationMethod: "transactionWatch_v1_watchEvent",
                terminalEvents: ["finalized", "error", "invalid", "dropped"],
                connectionLossEvent: .dictionaryValue([
                    "event": .stringValue("dropped"),
                    "error": .stringValue("host connection reset")
                ])
            )
        ),
        "archive_v1_storage": Family(
            unsubscribeMethod: "archive_v1_stopStorage",
            unsubscribeResult: .null,
            events: Events(
                notificationMethod: "archive_v1_storageEvent",
                terminalEvents: ["storageDone", "storageError"],
                connectionLossEvent: .dictionaryValue([
                    "event": .stringValue("storageError"),
                    "error": .stringValue("host connection reset")
                ])
            )
        ),
        "archive_v1_storageDiff": Family(
            unsubscribeMethod: "archive_v1_stopStorageDiff",
            unsubscribeResult: .null,
            events: Events(
                notificationMethod: "archive_v1_storageDiffEvent",
                terminalEvents: ["storageDiffDone", "storageDiffError"],
                connectionLossEvent: .dictionaryValue([
                    "event": .stringValue("storageDiffError"),
                    "error": .stringValue("host connection reset")
                ])
            )
        ),
        "statement_subscribeStatement": Family(
            unsubscribeMethod: "statement_unsubscribeStatement",
            unsubscribeResult: .bool,
            events: nil
        ),
        "author_submitAndWatchExtrinsic": Family(
            unsubscribeMethod: "author_unwatchExtrinsic",
            unsubscribeResult: .bool,
            events: nil
        ),
        "state_subscribeStorage": Family(
            unsubscribeMethod: "state_unsubscribeStorage",
            unsubscribeResult: .bool,
            events: nil
        ),
        "state_subscribeRuntimeVersion": Family(
            unsubscribeMethod: "state_unsubscribeRuntimeVersion",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeRuntimeVersion": Family(
            unsubscribeMethod: "chain_unsubscribeRuntimeVersion",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeNewHeads": Family(
            unsubscribeMethod: "chain_unsubscribeNewHeads",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeNewHead": Family(
            unsubscribeMethod: "chain_unsubscribeNewHead",
            unsubscribeResult: .bool,
            events: nil
        ),
        "subscribe_newHead": Family(
            unsubscribeMethod: "unsubscribe_newHead",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeAllHeads": Family(
            unsubscribeMethod: "chain_unsubscribeAllHeads",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeFinalizedHeads": Family(
            unsubscribeMethod: "chain_unsubscribeFinalizedHeads",
            unsubscribeResult: .bool,
            events: nil
        ),
        "chain_subscribeFinalisedHeads": Family(
            unsubscribeMethod: "chain_unsubscribeFinalisedHeads",
            unsubscribeResult: .bool,
            events: nil
        ),
        "grandpa_subscribeJustifications": Family(
            unsubscribeMethod: "grandpa_unsubscribeJustifications",
            unsubscribeResult: .bool,
            events: nil
        ),
        "beefy_subscribeJustifications": Family(
            unsubscribeMethod: "beefy_unsubscribeJustifications",
            unsubscribeResult: .bool,
            events: nil
        )
    ]

    private static let familiesByUnsubscribe: [String: Family] = Dictionary(
        uniqueKeysWithValues: families.values.map { ($0.unsubscribeMethod, $0) }
    )

    public static func family(forSubscribe method: String) -> Family? {
        families[method]
    }

    public static func family(forUnsubscribe method: String) -> Family? {
        familiesByUnsubscribe[method]
    }

    public static func isUnsubscribe(_ method: String) -> Bool {
        familiesByUnsubscribe[method] != nil
    }
}
