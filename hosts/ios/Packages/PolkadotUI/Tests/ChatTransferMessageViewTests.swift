import Testing
import UIKit
@testable import PolkadotUI

@MainActor
struct ChatTransferMessageViewTests {
    @Test("The amount row shows the currency symbol before the amount and the asset after it")
    func rendersCurrencyAndAsset() {
        let configuration = makeConfiguration()
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(view.amountView.amountLabel.text == "$20")
        #expect(view.amountView.unitLabel.text == "CASH")
        #expect(view.amountView.amountLabel.textColor == configuration.amountTextColor)
        #expect(view.amountView.unitLabel.textColor == configuration.tokenSymbolColor)
    }

    @Test("The row steps down the typography ladder as the amount grows")
    func scalesWithAmount() {
        #expect(makeView(amount: "20").amountView.typography == .headlineLarge)
        #expect(makeView(amount: "20,000").amountView.typography == .headlineSmall)
        #expect(makeView(amount: "2,000,000").amountView.typography == .titleExtraLarge)
    }

    @Test("The asset is set in the small caps face at the amount's size")
    func assetFollowsAmountSize() {
        let view = makeView(amount: "20")
        let amountFont = view.amountView.amountLabel.font
        let unitFont = view.amountView.unitLabel.font

        #expect(unitFont?.pointSize == amountFont?.pointSize)
        #expect(unitFont?.fontName == UIFont.app(.smallCapsHeadlineMedium).fontName)
    }

    @Test("A short claim strikes the original amount through and warns that it differs")
    func partialClaimShowsOriginalAmount() {
        let configuration = makeConfiguration(state: .outgoing(.claimed), originalAmountText: "50")
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(view.originalAmountLabel.isHidden == false)
        #expect(view.originalAmountLabel.attributedText?.string == "$50")
        #expect(view.subtitleLabel.text == String(localized: .transferStatusAmountDiffers))
        #expect(view.subtitleIconView.isHidden)
    }

    @Test("A failed transfer renders its status in the error tint")
    func failedRendersErrorTint() {
        let view = ChatTransferMessageView(configuration: makeConfiguration(state: .incoming(.failed)))

        #expect(view.subtitleLabel.text == String(localized: .transferStatusError))
        #expect(view.subtitleLabel.textColor == .fgError)
        #expect(view.originalAmountLabel.isHidden)
    }
}

private extension ChatTransferMessageViewTests {
    func makeView(amount: String) -> ChatTransferMessageView {
        ChatTransferMessageView(configuration: makeConfiguration(amount: amount))
    }

    func makeConfiguration(
        amount: String = "20",
        state: ChatTransferMessageConfiguration.DirectionalState = .outgoing(.sent),
        originalAmountText: String? = nil
    ) -> ChatTransferMessageConfiguration {
        ChatTransferMessageConfiguration(
            title: "You Sent",
            currencySymbol: "$",
            amountText: amount,
            tokenSymbol: "CASH",
            originalAmountText: originalAmountText,
            state: state,
            statusConfiguration: .init(
                dateFormatter: FixedTimestampFormatter(),
                date: .now,
                textColor: .fgPrimaryInverted,
                image: nil,
                isEdited: false
            ),
            backgroundColor: .bgSurfaceContainerInverted,
            titleColor: .fgPrimaryInverted,
            amountBackgroundColor: .bgSurfaceNestedInverted,
            amountTextColor: .fgPrimaryInverted,
            tokenSymbolColor: .fgSecondaryInverted,
            originalAmountTextColor: .fgSecondaryInverted,
            side: .trailing
        )
    }
}

private struct FixedTimestampFormatter: TimestampFormatting {
    func string(for _: Date, now _: Date) -> String { "2:33" }
}
