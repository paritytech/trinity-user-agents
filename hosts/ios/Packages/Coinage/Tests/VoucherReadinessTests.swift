import Foundation
import Testing
@testable import Coinage

struct VoucherReadinessTests {
    private let now = Date(timeIntervalSince1970: 1_000)

    @Test(arguments: [RecyclingStrategyType.balanced, .maxPrivacy])
    func waitingRequiresBothMembersAndConfirmedAge(_ type: RecyclingStrategyType) {
        let minimumMembers: UInt32 = 32
        let tenMinutes: TimeInterval = 10 * 60
        let strategy = strategy(type)
        let testCases: [(members: UInt32, age: TimeInterval?, expected: Bool)] = [
            (minimumMembers - 1, tenMinutes, false),
            (minimumMembers, tenMinutes - 1, false),
            (minimumMembers, tenMinutes, true),
            (minimumMembers, nil, false)
        ]

        for testCase in testCases {
            let candidate = voucher(
                members: testCase.members,
                enteredAt: testCase.age.map { now.addingTimeInterval(-$0) }
            )
            #expect(strategy.isVoucherUsable(candidate, context: context()) == testCase.expected)
        }
    }

    @Test
    func ringFillUsesNinetyAndTwentyPercentWithoutRoundingDown() {
        let capacity = 767
        let ninetyPercentMembers = UInt32((Double(capacity) * 0.9).rounded(.up))
        let twentyPercentMembers = UInt32((Double(capacity) * 0.2).rounded(.up))

        for (type, minimumMembers) in [
            (RecyclingStrategyType.maxPrivacy, ninetyPercentMembers),
            (.balanced, twentyPercentMembers)
        ] {
            let strategy = strategy(type)
            let testCases = [
                (members: minimumMembers - 1, expected: false),
                (members: minimumMembers, expected: true)
            ]

            for testCase in testCases {
                let candidate = voucher(members: testCase.members, enteredAt: nil)
                #expect(
                    strategy.isVoucherUsable(candidate, context: context(capacity: capacity))
                        == testCase.expected
                )
            }
        }
    }

    @Test
    func ringFillCanReleaseBeforeEitherWaitingRequirement() {
        let capacity = 10
        let ninetyPercentMembers = UInt32((Double(capacity) * 0.9).rounded(.up))

        #expect(strategy(.maxPrivacy).isVoucherUsable(
            voucher(members: ninetyPercentMembers, enteredAt: nil),
            context: context(capacity: capacity)
        ))
    }
}

private extension VoucherReadinessTests {
    func strategy(_ type: RecyclingStrategyType) -> ParametricRecyclingStrategy {
        ParametricRecyclingStrategy(params: type.params(forcedRecyclingAge: CoinageConstants.recycleAtAge))
    }

    func context(capacity: Int = 767) -> VoucherUsabilityContext {
        VoucherUsabilityContext(ringCapacities: [1: capacity], now: now)
    }

    func voucher(members: UInt32, enteredAt: Date?) -> Voucher {
        Voucher(
            exponent: 1,
            derivationIndex: 0,
            allocatedAt: .distantPast,
            readyAt: .distantPast,
            remoteState: .inRecycler(.init(index: 1, membersCount: members, enteredAt: enteredAt)),
            publicKey: Data(repeating: 0, count: 32)
        )
    }
}
