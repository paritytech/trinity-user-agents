import UIKit

protocol RootViewModelMaking {
    func makeInitial() -> RootViewLayout.ViewModel
    func makeLoadingHint() -> RootViewLayout.ViewModel
    func makeFailure(kind: RootSetupFailureKind) -> RootViewLayout.ViewModel
}

final class RootViewModelFactory: RootViewModelMaking {
    func makeInitial() -> RootViewLayout.ViewModel {
        .loading(hint: nil)
    }

    func makeLoadingHint() -> RootViewLayout.ViewModel {
        .loading(hint: String(localized: .rootInitLoadingHint))
    }

    func makeFailure(kind: RootSetupFailureKind) -> RootViewLayout.ViewModel {
        switch kind {
        case .connectivity:
            makeIssue(
                title: .rootInitFailureConnectivityTitle,
                subtitle: .rootInitFailureConnectivitySubtitle,
                actionTitle: nil
            )
        case let .configuration(stage):
            makeConfigurationIssue(for: stage)
        case .unknown:
            makeIssue(
                title: .rootInitFailureUnknownTitle,
                subtitle: .rootInitFailureUnknownSubtitle,
                actionTitle: .rootInitFailureAction
            )
        }
    }
}

private extension RootViewModelFactory {
    func makeConfigurationIssue(for stage: RootSetupStage) -> RootViewLayout.ViewModel {
        switch stage {
        case .config:
            makeIssue(
                title: .rootInitFailureConfigTitle,
                subtitle: .rootInitFailureConfigSubtitle,
                actionTitle: .rootInitFailureAction
            )
        case .chains:
            makeIssue(
                title: .rootInitFailureChainsTitle,
                subtitle: .rootInitFailureChainsSubtitle,
                actionTitle: .rootInitFailureAction
            )
        case .tld:
            makeIssue(
                title: .rootInitFailureTldTitle,
                subtitle: .rootInitFailureTldSubtitle,
                actionTitle: .rootInitFailureAction
            )
        }
    }

    func makeIssue(
        title: LocalizedStringResource,
        subtitle: LocalizedStringResource,
        actionTitle: LocalizedStringResource?
    ) -> RootViewLayout.ViewModel {
        .failed(
            RootViewLayout.ViewModel.Issue(
                title: String(localized: title),
                subtitle: String(localized: subtitle),
                actionTitle: actionTitle.map { String(localized: $0) }
            )
        )
    }
}
