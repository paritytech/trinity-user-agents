import Foundation
import PolkadotUI
import FoundationExt

protocol ClaimUsernameViewModelProviding {
    func viewModel() -> ClaimUsernameContentViewModel
}

final class ClaimUsernameViewModelFactory {
    let recoverable: Bool
    let full: Bool

    init(recoverable: Bool, full: Bool) {
        self.recoverable = recoverable
        self.full = full
    }
}

extension ClaimUsernameViewModelFactory: ClaimUsernameViewModelProviding {
    func viewModel() -> ClaimUsernameContentViewModel {
        if full {
            fullViewModel()
        } else if recoverable {
            recoverableViewModel()
        } else {
            recoveredViewModel()
        }
    }
}

private extension ClaimUsernameViewModelFactory {
    func recoveredViewModel() -> ClaimUsernameContentViewModel {
        ClaimUsernameContentViewModel(
            headerText: String(localized: .claimUsernameHeaderTitleRecovered),
            title: String(localized: .claimUsernameTitle),
            details: String(localized: .claimUsernameDetailsRecovered),
            actionTitle: String(localized: .claimUsernameAction),
            recoveryActionString: nil,
            termsActionString: nil
        )
    }

    func recoverableViewModel() -> ClaimUsernameContentViewModel {
        let defaultAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: .fgTertiary
        )
        let highlightingAttributes = LabelStyle.body14Regular().attributes(
            for: .center,
            textColor: .fgPrimary
        )

        let string = NSAttributedString.highlightedItems(
            [
                String(localized: .claimUsernameRecoverHere)
            ],
            formattingClosure: { items in
                String(localized: .claimUsernameRecoverAction(highlightedItem: items[0]))
            },
            highlightingAttributes: highlightingAttributes,
            defaultAttributes: defaultAttributes
        )

        return ClaimUsernameContentViewModel(
            headerText: String(localized: .claimUsernameHeaderTitle),
            title: String(localized: .claimUsernameTitle),
            details: String(localized: .claimUsernameDetails),
            actionTitle: String(localized: .claimUsernameAction),
            recoveryActionString: string,
            termsActionString: termsAttributedString()
        )
    }

    func fullViewModel() -> ClaimUsernameContentViewModel {
        ClaimUsernameContentViewModel(
            headerText: "",
            title: String(localized: .claimUsernameTitleFull),
            details: String(localized: .claimUsernameDetailsFull),
            actionTitle: String(localized: .claimUsernameActionFull),
            recoveryActionString: nil,
            termsActionString: nil
        )
    }

    func termsAttributedString() -> AttributedString {
        let termsText = String(localized: .claimUsernameTermsText)
        let privacyText = String(localized: .claimUsernamePrivacyText)
        let full = String(localized: .claimUsernameTermsOfUseAndPrivacyPolicy(termsText, privacyText))

        var attr = AttributedString(full)

        if let range = attr.range(of: termsText) {
            attr[range].link = AppConfig.termsOfUseLink
        }
        if let range = attr.range(of: privacyText) {
            attr[range].link = AppConfig.privacyPolicyLink
        }
        return attr
    }
}
