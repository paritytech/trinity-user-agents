import Foundation
import Testing

@testable import polkadot_app

/// Empty labels end the warm right after the lookup, so these tests observe only when the lookup starts.
@MainActor
@Suite(.timeLimit(.minutes(5)))
struct ProductContentPrewarmerTests {
    @Test func backgroundPrewarmWaitsForForeground() async {
        let activity = ApplicationActivityStub(isInBackground: true)
        let labelsProvider = ProductPrewarmLabelsProviderStub()
        let prewarmer = makePrewarmer(activity: activity, labelsProvider: labelsProvider)

        prewarmer.prewarm()
        await activity.changes.wait { activity.waitCallCount == 1 }

        #expect(labelsProvider.lookupCount == 0)

        activity.enterForeground()
        await labelsProvider.changes.wait { labelsProvider.lookupCount == 1 }

        #expect(labelsProvider.lookupCount == 1)
    }

    @Test func foregroundPrewarmRunsAtOnce() async {
        let activity = ApplicationActivityStub(isInBackground: false)
        let labelsProvider = ProductPrewarmLabelsProviderStub()
        let prewarmer = makePrewarmer(activity: activity, labelsProvider: labelsProvider)

        prewarmer.prewarm()
        await labelsProvider.changes.wait { labelsProvider.lookupCount == 1 }

        #expect(activity.waitCallCount == 0)
    }

    @Test func repeatedPrewarmWhileWaitingStartsOneWarm() async {
        let activity = ApplicationActivityStub(isInBackground: true)
        let labelsProvider = ProductPrewarmLabelsProviderStub()
        let prewarmer = makePrewarmer(activity: activity, labelsProvider: labelsProvider)

        prewarmer.prewarm()
        prewarmer.prewarm()
        await activity.changes.wait { activity.waitCallCount == 1 }
        activity.enterForeground()
        await labelsProvider.changes.wait { labelsProvider.lookupCount == 1 }

        #expect(activity.waitCallCount == 1)
    }
}

private extension ProductContentPrewarmerTests {
    func makePrewarmer(
        activity: ApplicationActivityStub,
        labelsProvider: ProductPrewarmLabelsProviderStub
    ) -> ProductContentPrewarmer {
        let chainRegistry = MockChainRegistry()
        chainRegistry.chainsOnSubscribe = [ChainMock.makeChain(id: AppConfig.Chains.assethubChain)]

        return ProductContentPrewarmer(
            labelsProvider: labelsProvider,
            chainRegistryClosure: { chainRegistry },
            flowStateProvider: UnreachableFlowStateProvider(),
            activity: activity,
            logger: StubLogger()
        )
    }
}
