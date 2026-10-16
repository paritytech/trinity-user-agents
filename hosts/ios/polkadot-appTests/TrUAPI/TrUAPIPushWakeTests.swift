import Foundation
import Testing
import Products
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// A wake push of the push relay reaches the core, and the app stays active while the core holds a worker
/// (VERBOSITY-WASM-PRODUCT A1).
struct TrUAPIPushWakeTests {
    private static let wake: [String: Any] = [
        "v": 1,
        "topic": "0x" + String(repeating: "01", count: 32),
        "statement": "0x" + String(repeating: "02", count: 32),
        "sentAt": 1_800_000_000_000,
        "mac": "0x" + String(repeating: "03", count: 32)
    ]

    /// The app gives the core the wake object as JSON text, and leaves every other push alone.
    @Test func onlyAWakePushHasAPayload() throws {
        let payload = try #require(TrUAPIPushWakeReceiver.payload(of: [
            "aps": ["content-available": 1],
            "truapi-wake": Self.wake
        ]))

        let decoded = try JSONSerialization.jsonObject(with: Data(payload.utf8)) as? NSDictionary
        #expect(decoded == Self.wake as NSDictionary)
        #expect(TrUAPIPushWakeReceiver.payload(of: ["aps": ["alert": "hello"]]) == nil)
    }

    /// The manager holds a background task for each product that the core holds, and none for the others.
    @Test func eachHeldProductHasABackgroundTaskUntilTheEndOfItsHold() async throws {
        let references = StubWorkerReferences()
        references.pushDeliveries = [
            PushWakeDelivery(
                productId: "a.dot",
                roomId: "home",
                watchId: "inbox",
                outcome: .held(untilMs: 1_800_000_030_000)
            ),
            PushWakeDelivery(productId: "b.dot", roomId: "main", watchId: "w", outcome: .running)
        ]
        let tasks = RecordingWakeTasks()
        let manager = TrUAPIWorkerManager(
            builder: NeverBuilder(),
            collection: InMemoryPocketCardStore([]),
            references: { references },
            backgroundTasks: tasks
        )

        let deliveries = try await manager.receiveWakePush(payload: "{}")

        #expect(deliveries.count == 2)
        #expect(references.log == ["push {}"])
        #expect(await tasks.holds == [
            RecordingWakeTasks.Hold(productId: "a.dot", deadline: Date(timeIntervalSince1970: 1_800_000_030))
        ])
        await manager.shutdown()
    }

    /// The fetch stays open until the hold ends, so iOS keeps the app running while the worker reads the store.
    @MainActor
    @Test func theFetchEndsWithTheHold() async {
        let workers = StubWorkerManager()
        let now = Date()
        workers.references.pushDeliveries = [
            PushWakeDelivery(
                productId: "a.dot",
                roomId: "home",
                watchId: "inbox",
                outcome: .held(untilMs: UInt64(now.addingTimeInterval(0.2).timeIntervalSince1970 * 1_000))
            )
        ]
        let receiver = TrUAPIPushWakeReceiver(workers: { workers })
        let started = ContinuousClock.now

        let result = await receiver.handle("{}")

        #expect(result == .newData)
        #expect(ContinuousClock.now - started >= .milliseconds(150))
    }

    @MainActor
    @Test func aRefusedPushOrNoSessionWakesNothing() async {
        let workers = StubWorkerManager()
        workers.references.pushDeliveries = nil

        #expect(await TrUAPIPushWakeReceiver(workers: { workers }).handle("{}") == .failed)
        #expect(await TrUAPIPushWakeReceiver(workers: { nil }).handle("{}") == .noData)
    }

    /// The registration has the fields the reference relay reads (`truapi-host wake-relay`).
    @Test func theRelayRegistrationNamesTheDeviceTheKeyAndTheTopics() throws {
        let client = TrUAPIWakeRelayClient(
            endpoint: URL(string: "http://127.0.0.1:9988")!,
            device: WakeRelayDevice(platform: "simctl", token: "UDID", app: "io.example")
        )

        let body = try client.body(
            for: WakeRelayRegistration(key: Data(repeating: 4, count: 32), topics: [Data(repeating: 1, count: 32)])
        )

        let json = try JSONSerialization.jsonObject(with: body) as? NSDictionary
        #expect(json == [
            "device": ["platform": "simctl", "token": "UDID", "app": "io.example"],
            "key": "0x" + String(repeating: "04", count: 32),
            "topics": ["0x" + String(repeating: "01", count: 32)]
        ] as NSDictionary)
    }

    @Test func onlyAHarnessWithARelayUrlHasARelay() {
        #expect(TrUAPIWakeRelayClient.configured(environment: [:], bundleId: "io.example") == nil)
        #if targetEnvironment(simulator)
            #expect(TrUAPIWakeRelayClient.configured(
                environment: ["TRUAPI_IOS_E2E_WAKE_RELAY_URL": "http://127.0.0.1:9988", "SIMULATOR_UDID": "UDID"],
                bundleId: "io.example"
            ) != nil)
        #endif
    }
}

/// A builder for a manager whose tests start no worker.
private struct NeverBuilder: TrUAPIWorkerBuilding {
    func makeRuntime(
        productId _: ProductId,
        context _: ProductWorkerContext,
        pocket _: ProductPocketHostBridge
    ) async throws -> TrUAPIWorkerRuntime {
        throw CancellationError()
    }
}
