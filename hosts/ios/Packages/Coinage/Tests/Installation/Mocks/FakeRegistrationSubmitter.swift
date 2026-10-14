import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// An accepted attempt shows up as a fresh pending row in its target's group; a gate holds it before it
/// commits.
final class FakeRegistrationSubmitter: InstallationRegistrationSubmitting, @unchecked Sendable {
    private let engine: FakeRegistrationEngine
    private let state = OSAllocatedUnfairLock<(attempts: [InstallationRegistrationTarget], failNext: Int)>(
        initialState: ([], 0)
    )
    private let gate: AsyncStream<Void>.Continuation
    private let gateStream: AsyncStream<Void>
    private var gated = false

    init(engine: FakeRegistrationEngine) {
        self.engine = engine
        (gateStream, gate) = AsyncStream<Void>.makeStream()
    }

    var attempts: [InstallationRegistrationTarget] { state.withLock { $0.attempts } }

    func failNextAttempts(_ count: Int) {
        state.withLock { $0.failNext = count }
    }

    /// Holds every attempt at its start until ``openGate()``.
    func holdAttempts() {
        gated = true
    }

    func openGate() {
        gated = false
        gate.yield(())
    }

    func submitAttempt(target: InstallationRegistrationTarget) async throws -> DurableTxId {
        state.withLock { $0.attempts.append(target) }

        if gated {
            for await _ in gateStream {
                break
            }
        }

        let shouldFail = state.withLock { state -> Bool in
            guard state.failNext > 0 else { return false }
            state.failNext -= 1
            return true
        }
        if shouldFail { throw InstallationStubError.notEnoughPgas }

        let entry = DurableTxEntry.registration(target, status: .pending)
        engine.set(engine.current(target.registrationGroup) + [entry], group: target.registrationGroup)
        return entry.id
    }
}
