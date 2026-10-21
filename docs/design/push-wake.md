---
title: "Push Wakes"
type: design
status: experimental
created: 2026-10-16
---

# Push wakes

A worker that follows statement-store topics (a chat that receives contact requests and messages) stops when the app
goes to the background. A push wake starts it again when a new statement comes on one of its topics:

- The worker names its topics with `worker.setWakeWatches`. The core keeps them.
- The core gives the topics of all products, with a key of the device, to the host's push relay.
- The relay follows the topics on the statement store. For each new statement it sends a wake push to the device.
- The host gives the push to the core. The core checks it and holds the worker of each product that watches the topic
  for 30 s, as a notification wake of the [Worker Lifecycle](../rfcs/worker-lifecycle.md).
- The worker reads the statement store itself and shows what came, for example with
  `notifications.sendPushNotification`.

```
 worker            core (in the app)                 host (app)              relay                  statement store
 setWakeWatches -> keeps watches in core storage
                   key + topics of all products ---> WakeRelay client ----> POST /v1/registrations
                                                                            follows the topics <--- new statement
                                                     push (APNs/FCM) <----- wake push: topic, hash, time, MAC
                   handle_wake_push(payload) <------ didReceiveRemoteNotification
                   checks MAC, age, replay, topic
                   wake_product(Notification) -> worker starts, reads the store, notifies
```

The product never sees the relay, the key or a push token, and it never reaches the relay or the statement store
outside TrUAPI. The relay is host infrastructure.

## The worker side

`Worker::set_wake_watches` (trait 19, method 2) replaces all watches of the product:

```rust
truapi::worker::set_wake_watches(HostWorkerSetWakeWatchesRequest {
    watches: vec![WorkerWakeWatch { id: "requests".into(), room_id: "home".into(), topics: vec![inbox] }],
}).await?.delivery // Push, or Unavailable { reason }
```

- A watch routes its wake to a chat room: the room gets `ChatRoomEvent::Woken { watch_id }`.
- Limits: 64 watches, 64 topics in one watch, 256 distinct topics in all. Ids and room ids are chat identifiers, and
  two watches cannot have one id. An empty list removes the watches.
- The answer is `Push` when the host's relay took the topics, and `Unavailable { reason }` when the host has no relay or
  the relay refused them. The core keeps the watches in both cases.
- Call it again when the topics change. Each call also refreshes the registration at the relay.
- A wake shows nothing by itself. The worker sends a notification with the room link as deeplink for what it read,
  as it does while it runs in the foreground; the host shows it by the "Room notifications" rule of
  [Chat Extension v2 additions](../rfcs/chat-extension-v2-additions.md). A worker that sends one notification for each
  event that it applies, once, shows each event once, whether a wake or the foreground worker reads it first.

## The core

`runtime/push_wake.rs`:

- **Registry.** The watches of every product and a random 32-byte key, in `CoreStorageKey::PushWakeRegistry`, so they
  stay while the worker is stopped and after the app restarts. The key is made with the first watch.
- **Relay client.** `WakeRelay::register_wake_topics(WakeRelayRegistration { key, topics })`, the sorted distinct topics
  of all products. A native host installs it with `NativeTrUApiHostRuntime::set_wake_relay`
  (`NativeWakeRelayCallbacks`, Swift `WakeRelayHostBridge`); the core then registers the stored topics at once. The
  host adds what identifies the device (push token, app id) and calls its relay endpoint. The core waits 15 s for an
  answer.
- **Push check.** `handle_wake_push(payload)` takes the JSON object under the key `truapi-wake` of the push:

  ```json
  {"v": 1, "topic": "0x…32 bytes", "statement": "0x…32 bytes", "sentAt": 1792124406283, "mac": "0x…32 bytes"}
  ```

  `statement` is BLAKE2b-256 of the SCALE-encoded statement; `mac` is keyed BLAKE2b-256 with the device key over
  `"truapi/wake-push/v1" ‖ topic ‖ statement ‖ sentAt (u64 LE)`. The core refuses (`PushWakeRejection`) a push that is
  malformed, has a bad MAC, is older than 15 minutes or more than 5 minutes ahead, repeats a statement of the last 512
  pushes, or names a topic that no product watches. It wakes nothing then.
- **Wake.** For each product and watch with the topic, `wake_product(Notification { room_id, watch_id })`: a hold of
  30 s, at most 12 notification wakes per product in one hour (`PushWakeOutcome::Held` or `BudgetSpent`). When the app
  is in the foreground and the product declares a foreground worker, the worker runs already and reads the statement
  itself: `Running`, and no budget goes.

The push carries no statement content. The relay learns which topics a device follows, not what is said on them.

## The relay contract

The reference relay is `truapi-host wake-relay` (`rust/crates/truapi-host-cli/src/wake_relay.rs`). A production relay
implements the same contract with push credentials.

- `POST /v1/registrations` with
  `{"device": {"platform": "apns" | "apns-sandbox" | "fcm" | "simctl", "token": "…", "app": "<bundle or package>"},
  "key": "0x…", "topics": ["0x…"]}` replaces the registration of the device; empty `topics` removes it.
- The relay follows the topics with `statement_subscribeStatement` (`matchAny`, 128 topics per subscription). It sends
  no push for the backlog that a new subscription replays, at most one push per statement, and no second push for a
  topic of a device within a quiet time (the wake of the first push reads every statement).
- The push is a background push with the payload `{"aps": {"content-available": 1}, "truapi-wake": {…}}`.

## iOS

- `AppDelegate.application(_:didReceiveRemoteNotification:fetchCompletionHandler:)` gives a push with `truapi-wake` to
  `TrUAPIPushWakeReceiver`, which calls `TrUAPIWorkerManaging.receiveWakePush`. The manager calls the core, and for
  each held product starts a faulted worker again and holds a UIKit background task until the end of the hold. The
  receiver keeps the fetch open until the last hold ends, at most 28 s.
- `TrUAPIWakeRelayClient` is the relay client. It is installed only where an endpoint is configured; today that is the
  E2E harness on a simulator (`TRUAPI_IOS_E2E_WAKE_RELAY_URL`, device platform `simctl`).

## Limits and gaps

- **No production relay.** No relay holds APNs or FCM credentials for the apps (open item O27), and the receiving layer
  of `feat/background-notification-receiving` is not on this branch (O28). This design does not depend on it, but the
  two must be reconciled when it merges.
- **Simulator.** `xcrun simctl push` cannot deliver a background push: it refuses a payload with no visible content
  and does not call the app delegate. The E2E harness therefore uses the relay's `simulator` sink and
  `TrUAPIE2EWakePushFeed`, which hands the payload to the app delegate. While the feed is on, the app plays silence in
  the background so that the simulator does not suspend it. This stands in for the step where iOS resumes the app.
- **Cold launch.** A background launch connects no scene, so the app builds no TrUAPI session and a push to a
  terminated app wakes nothing (`wake push before the TrUAPI session exists`). A headless session bootstrap is needed.
- **Budget and throttling.** iOS throttles background pushes, and a busy chat can spend 12 wakes in an hour (O30).
- **Account.** The registry belongs to the install, not to one account; a product sets new watches after an account
  change.
- **Consent.** The relay learns the topics a device follows. No prompt asks the user before the core registers them.
