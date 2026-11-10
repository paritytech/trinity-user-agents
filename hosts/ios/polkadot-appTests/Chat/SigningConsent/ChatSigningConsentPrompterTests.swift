import Foundation
import os
import SubstrateSdk
import Testing
@testable import polkadot_app

@Suite(.timeLimit(.minutes(10)))
struct ChatSigningConsentPrompterTests {
    private let presenter = ConsentRequestsMock()
    private let alice = ChatSigningConsentSubject(contactAccountId: Data(repeating: 0x0A, count: 32), username: "alice")
    private let bob = ChatSigningConsentSubject(contactAccountId: Data(repeating: 0x0B, count: 32), username: "bob")

    @Test func requestsForOneContactShareOnePrompt() async {
        let prompter = ChatSigningConsentPrompter(presenter: presenter)

        let first = prompter.consentTask(for: alice)
        let second = prompter.consentTask(for: alice)
        await presenter.waitForRequests(1)
        presenter.flush(answer: true)

        let answers = await [first.value, second.value]

        #expect(answers == [true, true])
        #expect(presenter.requests.map(\.username) == ["alice"])
    }

    @Test func promptsForDifferentContactsOneAtATime() async {
        let prompter = ChatSigningConsentPrompter(presenter: presenter)

        let aliceAnswer = prompter.consentTask(for: alice)
        let bobAnswer = prompter.consentTask(for: bob)
        await presenter.waitForRequests(1)
        presenter.flush(answer: false)
        await presenter.waitForRequests(2)
        presenter.flush(answer: true)

        let answers = await ["alice": aliceAnswer.value, "bob": bobAnswer.value]
        let promptOrder = presenter.requests.compactMap(\.username)

        #expect(presenter.maxOpenRequests == 1)
        #expect(Set(promptOrder) == ["alice", "bob"])
        #expect(promptOrder.map { answers[$0] } == [false, true])
    }

    @Test func nextRequestAfterAnswerPromptsAgain() async {
        let prompter = ChatSigningConsentPrompter(presenter: presenter)

        let first = prompter.consentTask(for: alice)
        await presenter.waitForRequests(1)
        presenter.flush(answer: false)
        _ = await first.value

        let second = prompter.consentTask(for: alice)
        await presenter.waitForRequests(2)
        presenter.flush(answer: true)

        #expect(await second.value)
        #expect(presenter.requests.count == 2)
    }
}

/// Collects consent requests as they are added; `flush(answer:)` answers every open one.
private final class ConsentRequestsMock: ChatSigningConsentPresenting, @unchecked Sendable {
    fileprivate struct State {
        var requests: [ChatSigningConsentSubject] = []
        var open: [CheckedContinuation<Bool, Never>] = []
        var maxOpenRequests = 0
        var countWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())

    var requests: [ChatSigningConsentSubject] { state.withLock { $0.requests } }
    var maxOpenRequests: Int { state.withLock { $0.maxOpenRequests } }

    func askConsent(for subject: ChatSigningConsentSubject) async -> Bool {
        await withCheckedContinuation { continuation in
            let reached = state.withLock { state in
                state.requests.append(subject)
                state.open.append(continuation)
                state.maxOpenRequests = max(state.maxOpenRequests, state.open.count)
                return state.takeWaiters(reachedBy: state.requests.count)
            }
            reached.forEach { $0.resume() }
        }
    }

    func waitForRequests(_ count: Int) async {
        await withCheckedContinuation { continuation in
            let isReached = state.withLock { state in
                guard state.requests.count < count else { return true }
                state.countWaiters.append((count, continuation))
                return false
            }
            if isReached {
                continuation.resume()
            }
        }
    }

    func flush(answer: Bool) {
        let open = state.withLock { state in
            defer { state.open = [] }
            return state.open
        }
        open.forEach { $0.resume(returning: answer) }
    }
}

private extension ConsentRequestsMock.State {
    mutating func takeWaiters(reachedBy count: Int) -> [CheckedContinuation<Void, Never>] {
        let reached = countWaiters.filter { $0.count <= count }.map(\.continuation)
        countWaiters.removeAll { $0.count <= count }
        return reached
    }
}
