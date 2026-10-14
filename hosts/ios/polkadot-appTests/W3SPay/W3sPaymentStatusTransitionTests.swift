import Testing

@testable import polkadot_app

@Suite("W3sPaymentRecord.Status.allowsTransition")
struct W3sPaymentStatusTransitionTests {
    @Test("Status transitions follow the exact monotonic lifecycle matrix")
    func statusTransitionMatrix() {
        // All 6 possible statuses
        let allStatuses: [W3sPaymentRecord.Status] = [
            .pending,
            .submitted,
            .sent,
            .claimed,
            .failed(reason: "test"),
            .revoked
        ]

        // Truth table: (from, to) => allowed
        // - pending → all true
        // - submitted → all true
        // - sent → {claimed, revoked}: true, others: false
        // - claimed → all false
        // - failed → all true
        // - revoked → all false

        let transitionMatrix: [String: [String: Bool]] = [
            "pending": [
                "pending": true,
                "submitted": true,
                "sent": true,
                "claimed": true,
                "failed": true,
                "revoked": true
            ],
            "submitted": [
                "pending": true,
                "submitted": true,
                "sent": true,
                "claimed": true,
                "failed": true,
                "revoked": true
            ],
            "sent": [
                "pending": false,
                "submitted": false,
                "sent": false,
                "claimed": true,
                "failed": false,
                "revoked": true
            ],
            "claimed": [
                "pending": false,
                "submitted": false,
                "sent": false,
                "claimed": false,
                "failed": false,
                "revoked": false
            ],
            "failed": [
                "pending": true,
                "submitted": true,
                "sent": true,
                "claimed": true,
                "failed": true,
                "revoked": true
            ],
            "revoked": [
                "pending": false,
                "submitted": false,
                "sent": false,
                "claimed": false,
                "failed": false,
                "revoked": false
            ]
        ]

        // Test each (from, to) pair
        for fromStatus in allStatuses {
            let fromKey = statusKey(fromStatus)

            for toStatus in allStatuses {
                let toKey = statusKey(toStatus)
                let expected = transitionMatrix[fromKey]?[toKey] ?? false

                #expect(
                    fromStatus.canTransition(to: toStatus) == expected,
                    "Transition from \(fromKey) to \(toKey) should be \(expected)"
                )
            }
        }
    }

    @Test("Failed status with different reasons behaves identically (reason is ignored)")
    func failedReasonIgnored() {
        let failedWithMessage = W3sPaymentRecord.Status.failed(reason: "connection timeout")
        let failedWithNil = W3sPaymentRecord.Status.failed(reason: nil)

        // Both should allow same transitions when used as source
        let targetStatuses: [W3sPaymentRecord.Status] = [
            .pending,
            .submitted,
            .sent,
            .claimed,
            .failed(reason: "other reason"),
            .revoked
        ]

        for target in targetStatuses {
            #expect(
                failedWithMessage.canTransition(to: target) ==
                    failedWithNil.canTransition(to: target),
                "Failed reason should not affect transition logic for target \(statusKey(target))"
            )
        }

        // Both should allow same transitions when used as target
        let sourceStatuses: [W3sPaymentRecord.Status] = [
            .pending,
            .submitted,
            .sent,
            .claimed,
            .failed(reason: "first failure"),
            .revoked
        ]

        for source in sourceStatuses {
            #expect(
                source.canTransition(to: failedWithMessage) ==
                    source.canTransition(to: failedWithNil),
                "Failed reason should not affect transition logic from \(statusKey(source))"
            )
        }
    }

    private func statusKey(_ status: W3sPaymentRecord.Status) -> String {
        switch status {
        case .pending:
            "pending"
        case .submitted:
            "submitted"
        case .sent:
            "sent"
        case .claimed:
            "claimed"
        case .failed:
            "failed"
        case .revoked:
            "revoked"
        }
    }
}
