import Foundation
import PolkadotUI
import StructuredConcurrency
import Testing

@testable import polkadot_app

private func makeStep(
    id: String,
    title: String,
    depth: Int = 0,
    state: StallStep.State,
    elapsed: TimeInterval = 0
) -> StallReportSnapshot.Step {
    StallReportSnapshot.Step(id: id, title: title, depth: depth, state: state, elapsed: elapsed)
}

private func makeActivity(
    id: UUID = UUID(),
    title: String,
    subtitle: String? = nil,
    elapsed: TimeInterval = 0,
    steps: [StallReportSnapshot.Step]
) -> StallReportSnapshot.Activity {
    StallReportSnapshot.Activity(id: id, title: title, subtitle: subtitle, elapsed: elapsed, steps: steps)
}

@Suite("StallBannerViewModelFactory")
struct StallBannerViewModelFactoryTests {
    let factory = StallBannerViewModelFactory(maxVisibleRows: 2)

    @Test("nil snapshot produces no view model")
    func nilSnapshotProducesNil() {
        let viewModel = factory.createViewModel(from: nil)
        #expect(viewModel == nil)
    }

    @Test("snapshot with no activities produces no view model")
    func emptyActivitiesProducesNil() {
        let snapshot = StallReportSnapshot(activities: [])
        let viewModel = factory.createViewModel(from: snapshot)
        #expect(viewModel == nil)
    }

    @Test("one activity produces one transaction with no overflow")
    func oneActivityProducesOneTransaction() {
        let activity = makeActivity(
            title: "0x1234…5678",
            steps: [makeStep(id: "step1", title: "Step 1", state: .running)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.count == 1)
        #expect(viewModel?.overflowText == nil)
    }

    @Test("two activities produce two transactions preserving order")
    func twoActivitiesPreserveOrder() {
        let id1 = UUID()
        let id2 = UUID()
        let activity1 = makeActivity(id: id1, title: "tx1", steps: [])
        let activity2 = makeActivity(id: id2, title: "tx2", steps: [])
        let snapshot = StallReportSnapshot(activities: [activity1, activity2])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.map(\.id) == [id1, id2])
        #expect(viewModel?.overflowText == nil)
    }

    @Test("four activities with maxVisibleRows 2 produce overflow text")
    func fourActivitiesProduceOverflow() {
        let activities = (0 ..< 4).map { idx in
            makeActivity(title: "tx\(idx)", steps: [])
        }
        let snapshot = StallReportSnapshot(activities: activities)
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.count == 2)
        #expect(viewModel?.overflowText != nil)
    }

    @Test("dismissAccessibilityLabel is populated on every transaction")
    func dismissAccessibilityLabelAlwaysPopulated() {
        let activity = makeActivity(
            title: "0x1234…5678",
            steps: [makeStep(id: "step1", title: "Step 1", state: .running)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.dismissAccessibilityLabel != nil)
    }

    @Test("chainName comes from Activity subtitle")
    func chainNameFromSubtitle() {
        let activity = makeActivity(
            title: "0x1234…5678",
            subtitle: "Polkadot",
            steps: []
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.chainName == "Polkadot")
    }

    @Test("step running state maps to current")
    func runningStateMapsToCurrent() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .running)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.state == .current)
    }

    @Test("step finished state maps to done")
    func finishedStateMapsToDone() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .finished)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.state == .done)
    }

    @Test("step skipped state maps to skipped")
    func skippedStateMapToSkipped() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .skipped)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.state == .skipped)
    }

    @Test("step failed state maps to failed")
    func failedStateMapToFailed() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .failed(detail: "Error"))]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.state == .failed)
    }

    @Test("failed step detail is passed through unchanged")
    func failedDetailPassedThrough() {
        let detail = "Custom error message"
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .failed(detail: detail))]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.detail == detail)
    }

    @Test("failed step with nil detail keeps nil detail")
    func failedDetailNilPreserved() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "s1", title: "Step 1", state: .failed(detail: nil))]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.detail == nil)
    }

    @Test("stage id and text come from Step id and title")
    func stageIdAndTextFromStep() {
        let activity = makeActivity(
            title: "tx",
            steps: [makeStep(id: "myStepId", title: "My Step Title", state: .running)]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.first?.id == "myStepId")
        #expect(viewModel?.transactions.first?.stages.first?.text == "My Step Title")
    }

    @Test("factory passes step depth through")
    func factoryPassesStepDepthThrough() {
        let activity = makeActivity(
            title: "tx",
            steps: [
                makeStep(id: "s1", title: "Step 1", depth: 0, state: .running),
                makeStep(id: "s2", title: "Step 2", depth: 1, state: .finished)
            ]
        )
        let snapshot = StallReportSnapshot(activities: [activity])
        let viewModel = factory.createViewModel(from: snapshot)

        #expect(viewModel?.transactions.first?.stages.map(\.depth) == [0, 1])
    }

    @Test("factory ignores step elapsed")
    func factoryIgnoresStepElapsed() {
        let activity1 = makeActivity(
            title: "tx",
            steps: [
                makeStep(id: "s1", title: "Step 1", depth: 0, state: .running, elapsed: 0),
                makeStep(id: "s2", title: "Step 2", depth: 1, state: .finished, elapsed: 0)
            ]
        )
        let activity2 = makeActivity(
            title: "tx",
            steps: [
                makeStep(id: "s1", title: "Step 1", depth: 0, state: .running, elapsed: 100),
                makeStep(id: "s2", title: "Step 2", depth: 1, state: .finished, elapsed: 200)
            ]
        )
        let snapshot1 = StallReportSnapshot(activities: [activity1])
        let snapshot2 = StallReportSnapshot(activities: [activity2])
        let viewModel1 = factory.createViewModel(from: snapshot1)
        let viewModel2 = factory.createViewModel(from: snapshot2)

        #expect(viewModel1?.transactions.first?.stages == viewModel2?.transactions.first?.stages)
    }
}
