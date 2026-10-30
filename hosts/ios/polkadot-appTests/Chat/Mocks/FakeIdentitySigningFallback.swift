import Foundation
import os
import StatementStore
import SubstrateSdk
@testable import polkadot_app

/// Returns `signer` (or declines) and records which contacts asked for the identity fallback.
final class FakeIdentitySigningFallback: NonPrivateSigningFallbacking, @unchecked Sendable {
    private struct State {
        var contacts: [AccountId] = []
        var failure: Error?
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    let signer: StatementStoreSigning

    var contacts: [AccountId] { state.withLock { $0.contacts } }

    init(signer: StatementStoreSigning) {
        self.signer = signer
    }

    func decline() {
        fail(with: ChatSigningConsentError.declined)
    }

    func fail(with error: Error) {
        state.withLock { $0.failure = error }
    }

    func identitySigner(for contactAccountId: AccountId, ownSignKeyId _: String) async throws -> StatementStoreSigning {
        let failure = state.withLock { state in
            state.contacts.append(contactAccountId)
            return state.failure
        }

        if let failure {
            throw failure
        }

        return signer
    }
}
