import Testing
@testable import Individuality

struct NotificationParametersProviderTests {
    @Test func periodFollowsRuntimeDurationRatherThanDay() throws {
        #expect(try NotificationParametersProvider.period(atSeconds: 7_200, duration: 3_600) == 2)
        #expect(try NotificationParametersProvider.period(atSeconds: 86_399, duration: 86_400) == 0)
    }

    @Test func rejectsZeroDuration() {
        #expect(throws: NotificationParametersError.zeroPeriodDuration) {
            try NotificationParametersProvider.period(atSeconds: 1, duration: 0)
        }
    }

    @Test func maxStatementSizeFollowsAllowanceSetOnChain() async throws {
        let provider = makeProvider(onChain: ResourcesPallet.StatementAllowance(maxCount: 1, maxSize: 30 * 1_024))

        #expect(try await provider.maxStatementSize() == 30 * 1_024)
    }

    @Test func maxStatementSizeFallsBackToRuntimeDefaultWhileParameterIsUnset() async throws {
        let provider = makeProvider(onChain: nil)
        let expected = Int(ResourcesPallet.StatementAllowance.notificationDefault.maxSize)

        #expect(try await provider.maxStatementSize() == expected)
    }
}

private extension NotificationParametersProviderTests {
    func makeProvider(onChain allowance: ResourcesPallet.StatementAllowance?) -> NotificationParametersProvider {
        NotificationParametersProvider(
            chainId: "people",
            chainRegistry: UnavailableChainResource(),
            chainTimeProvider: FakeChainTimeProvider(),
            statementStorageParameters: FixedStatementStorageParameters(allowance: allowance),
            logger: FakeLogger()
        )
    }
}
