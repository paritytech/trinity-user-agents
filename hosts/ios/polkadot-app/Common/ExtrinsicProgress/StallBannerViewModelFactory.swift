#if TESTNET_FEATURE
    import Foundation
    import PolkadotUI
    import StructuredConcurrency

    protocol StallBannerViewModelFactoryProtocol {
        func createViewModel(from snapshot: StallReportSnapshot?) -> StallBannerViewModel?
    }

    final class StallBannerViewModelFactory: StallBannerViewModelFactoryProtocol {
        static let defaultMaxVisibleRows = 3

        private let maxVisibleRows: Int

        init(maxVisibleRows: Int = defaultMaxVisibleRows) {
            self.maxVisibleRows = maxVisibleRows
        }

        func createViewModel(from snapshot: StallReportSnapshot?) -> StallBannerViewModel? {
            guard let snapshot, !snapshot.activities.isEmpty else {
                return nil
            }

            // StallBoard already sorts by startedAt ascending; do not re-sort.
            let transactions = snapshot.activities.prefix(maxVisibleRows).map { activity in
                StallBannerViewModel.Transaction(
                    id: activity.id,
                    title: activity.title,
                    chainName: activity.subtitle,
                    stages: stages(for: activity),
                    dismissAccessibilityLabel: String(localized: .ExtrinsicProgress.dismiss)
                )
            }

            let overflowCount = snapshot.activities.count - maxVisibleRows
            let overflowText = overflowCount > 0
                ? String(localized: .ExtrinsicProgress.moreCount(overflowCount))
                : nil

            return StallBannerViewModel(transactions: transactions, overflowText: overflowText)
        }
    }

    private extension StallBannerViewModelFactory {
        func stages(for activity: StallReportSnapshot.Activity) -> [StallBannerViewModel.Stage] {
            // Phase 3 will use Step.elapsed; the factory carries no domain knowledge.
            activity.steps.map { step in
                let state: StallBannerViewModel.Stage.State =
                    switch step.state {
                    case .running:
                        .current
                    case .finished:
                        .done
                    case .failed:
                        .failed
                    case .skipped:
                        .skipped
                    }

                let detail: String? =
                    if case let .failed(detail) = step.state {
                        detail
                    } else {
                        nil
                    }

                return StallBannerViewModel.Stage(
                    id: step.id,
                    depth: step.depth,
                    text: step.title,
                    state: state,
                    detail: detail
                )
            }
        }
    }
#endif
