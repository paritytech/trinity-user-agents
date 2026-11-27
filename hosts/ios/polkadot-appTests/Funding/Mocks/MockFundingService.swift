import AsyncExtensions
import Foundation
import SubstrateSdk
import TrUAPIHost
@testable import polkadot_app

/// The core as the funding screens' interactors see it: records every ask in
/// order, answers with what the test set, and pushes the sessions the test
/// hands it to every screen following them.
final class MockFundingService: FundingServiceProtocol, @unchecked Sendable {
    enum Call: Equatable {
        case select(FundingSelection)
        case quote(Balance?)
        case start(ProductId?)
        case stopStarting
        case leave
        case cancel
        case answerCancelOffer(CancelTerms?)
        case acknowledge(String)
    }

    let events = AsyncStream<FundingEvent>.makeStream().stream.eraseToAnyAsyncSequence()
    /// Signals every recorded ask and every new follower.
    let changes = StateChangeSignal()

    private let lock = NSLock()
    private var recorded: [Call] = []
    private var amountIssueAsks: [(amount: Balance, spendable: Balance?)] = []
    private var entryFollowers: [AsyncThrowingStream<FundingEntry, Error>.Continuation] = []
    private var listFollowers: [AsyncThrowingStream<[FundingEntry], Error>.Continuation] = []
    private var heldCalls: [CheckedContinuation<Void, Never>] = []

    /// What the core says about every typed amount.
    var amountIssueAnswer: FundingAmountIssue? {
        get { locked { _amountIssueAnswer } }
        set { locked { _amountIssueAnswer = newValue } }
    }

    /// How each cancel ends, in order; a cancel past the list is `.cancelled`.
    var cancelResults: [Result<CancelOutcome, Error>] {
        get { locked { _cancelResults } }
        set { locked { _cancelResults = newValue } }
    }

    var startResult: Result<Void, Error> {
        get { locked { _startResult } }
        set { locked { _startResult = newValue } }
    }

    var cancelOfferAnswer: Result<CancelConfirmation, Error> {
        get { locked { _cancelOfferAnswer } }
        set { locked { _cancelOfferAnswer = newValue } }
    }

    /// Starts and cancels wait for `releaseHeldCalls()`.
    var holdsCalls: Bool {
        get { locked { _holdsCalls } }
        set { locked { _holdsCalls = newValue } }
    }

    private var _amountIssueAnswer: FundingAmountIssue?
    private var _cancelResults: [Result<CancelOutcome, Error>] = []
    private var _startResult: Result<Void, Error> = .success(())
    private var _cancelOfferAnswer: Result<CancelConfirmation, Error> = .success(.declined)
    private var _holdsCalls = false

    var calls: [Call] { locked { recorded } }
    var spendableAsked: [Balance?] { locked { amountIssueAsks.map(\.spendable) } }
    var entryStateSubscriptions: Int { locked { entryFollowers.count } }
    var heldCallCount: Int { locked { heldCalls.count } }

    /// Hands `entry` to every screen following its session.
    func push(_ entry: FundingEntry) {
        locked { entryFollowers }.forEach { $0.yield(entry) }
    }

    /// Hands `entries` to every screen following the list.
    func push(entries: [FundingEntry]) {
        locked { listFollowers }.forEach { $0.yield(entries) }
    }

    func releaseHeldCalls() {
        let held = locked {
            defer { heldCalls = [] }
            return heldCalls
        }
        held.forEach { $0.resume() }
    }

    // MARK: FundingServiceProtocol

    func subscribeEntries() -> AnyAsyncSequence<[FundingEntry]> {
        let (stream, continuation) = AsyncThrowingStream<[FundingEntry], Error>.makeStream()
        locked { listFollowers.append(continuation) }
        changes.signal()
        return stream.eraseToAnyAsyncSequence()
    }

    func subscribeEntryState(_: String) -> AnyAsyncSequence<FundingEntry> {
        let (stream, continuation) = AsyncThrowingStream<FundingEntry, Error>.makeStream()
        locked { entryFollowers.append(continuation) }
        changes.signal()
        return stream.eraseToAnyAsyncSequence()
    }

    func open(_: FundingDirection, amount _: Balance?) async throws -> String? {
        nil
    }

    func select(_ selection: FundingSelection, for _: String) {
        record(.select(selection))
    }

    func quote(amount: Balance?, for _: String) {
        record(.quote(amount))
    }

    func amountIssue(_ amount: Balance, spendable: Balance?, for _: String) -> FundingAmountIssue? {
        locked {
            amountIssueAsks.append((amount, spendable))
            return _amountIssueAnswer
        }
    }

    func start(_: String, provider: ProductId?) async throws {
        record(.start(provider))
        await waitIfHeld()
        try startResult.get()
    }

    func stopStarting(_: String) {
        record(.stopStarting)
    }

    func leave(_: String) {
        record(.leave)
    }

    func cancel(_: String) async throws -> CancelOutcome {
        record(.cancel)
        await waitIfHeld()
        let result: Result<CancelOutcome, Error> = locked {
            _cancelResults.isEmpty ? .success(.cancelled) : _cancelResults.removeFirst()
        }
        return try result.get()
    }

    func answerCancelOffer(_: String, accepting terms: CancelTerms?) async throws -> CancelConfirmation {
        record(.answerCancelOffer(terms))
        return try cancelOfferAnswer.get()
    }

    func acknowledge(_ entryId: String) async {
        record(.acknowledge(entryId))
    }

    func openProviderFrame(_: FundingFrameRequest) {}

    func waitingFrames() async -> [FundingFrameRequest] {
        []
    }

    func answer(frame _: FundingFrame.ID, with _: FundingFrameOutcome) {}
}

private extension MockFundingService {
    func locked<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    func record(_ call: Call) {
        locked { recorded.append(call) }
        changes.signal()
    }

    func waitIfHeld() async {
        guard holdsCalls else { return }

        await withCheckedContinuation { continuation in
            locked { heldCalls.append(continuation) }
            changes.signal()
        }
    }
}
