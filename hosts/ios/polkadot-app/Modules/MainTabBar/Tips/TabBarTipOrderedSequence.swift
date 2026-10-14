import TipKit

@MainActor
final class TabBarTipOrderedSequence: TabBarTipSequenceProtocol {
    private let steps: [TabBarTipStep]

    var currentStep: TabBarTipStep? {
        steps.first { $0.tip.shouldDisplay }
    }

    init(steps: [TabBarTipStep]) {
        self.steps = steps
    }

    func observeChanges(_ onChange: @escaping @MainActor () -> Void) async {
        await withTaskGroup(of: Void.self) { group in
            for step in steps {
                group.addTask { @MainActor in
                    for await _ in step.tip.shouldDisplayUpdates {
                        onChange()
                    }
                }
            }
        }
    }
}
