import Foundation
import Products

@testable import polkadot_app

/// Records the messages the processing context hands it. A message whose id is
/// in `gatedIds` is held until `release(_:)`, so a test can keep the queue busy;
/// when it finishes, the handler notes whether its task had been cancelled. It
/// also keeps the prompt scope each message was handled under.
actor GatedRequestHandler<Message: HostMessageIdentifiable>: SSORequestHandling {
    private let gatedIds: Set<String>

    private(set) var startedIds: [String] = []
    private(set) var finishedIds: [String] = []
    private(set) var cancelledIds: [String] = []
    private(set) var promptScopes: [String: PromptPresentationScope] = [:]

    private var gates: [String: CheckedContinuation<Void, Never>] = [:]
    private var releasedIds: Set<String> = []
    private var startWaiters: [String: [CheckedContinuation<Void, Never>]] = [:]
    private var finishWaiters: [String: [CheckedContinuation<Void, Never>]] = [:]

    init(gatedIds: Set<String> = []) {
        self.gatedIds = gatedIds
    }

    nonisolated func canHandle(_: Message) -> Bool { true }

    func handle(message: Message, from _: PolkadotSignInHost) async {
        let messageId = message.messageId
        startedIds.append(messageId)
        promptScopes[messageId] = PromptPresentationScope.current
        startWaiters.removeValue(forKey: messageId)?.forEach { $0.resume() }

        if gatedIds.contains(messageId), !releasedIds.contains(messageId) {
            await withCheckedContinuation { gates[messageId] = $0 }
        }

        if Task.isCancelled {
            cancelledIds.append(messageId)
        }

        finishedIds.append(messageId)
        finishWaiters.removeValue(forKey: messageId)?.forEach { $0.resume() }
    }

    func release(_ messageId: String) {
        releasedIds.insert(messageId)
        gates.removeValue(forKey: messageId)?.resume()
    }

    func waitUntilStarted(_ messageId: String) async {
        guard !startedIds.contains(messageId) else { return }
        await withCheckedContinuation { startWaiters[messageId, default: []].append($0) }
    }

    func waitUntilFinished(_ messageId: String) async {
        guard !finishedIds.contains(messageId) else { return }
        await withCheckedContinuation { finishWaiters[messageId, default: []].append($0) }
    }
}
