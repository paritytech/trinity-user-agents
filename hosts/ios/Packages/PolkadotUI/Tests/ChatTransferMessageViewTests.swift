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
        #expect(view.amountView.unitLabel.text == CashLogo.Wordmark.smallCaps.glyph)
        #expect(view.amountView.unitLabel.accessibilityLabel == "CASH")
        #expect(view.amountView.amountLabel.textColor == configuration.amountTextColor)
        #expect(view.amountView.unitLabel.textColor == configuration.tokenSymbolColor)
    }

    @Test("The row steps down the typography ladder as the amount grows")
    func scalesWithAmount() {
        #expect(makeView(amount: "$20").amountView.typography == .headlineLarge)
        #expect(makeView(amount: "$20,000").amountView.typography == .headlineSmall)
        #expect(makeView(amount: "$2,000,000").amountView.typography == .titleExtraLarge)
    }

    @Test("The asset is drawn as the CASH wordmark at the amount's size")
    func assetFollowsAmountSize() {
        let view = makeView(amount: "$20")
        let amountFont = view.amountView.amountLabel.font
        let unitFont = view.amountView.unitLabel.font

        #expect(unitFont?.pointSize == amountFont?.pointSize)
        #expect(unitFont?.fontName == CashLogo.font(size: 1).fontName)
    }

    @Test("A short claim strikes the original amount through and warns that it differs")
    func partialClaimShowsOriginalAmount() {
        let configuration = makeConfiguration(state: .outgoing(.claimed), originalAmountText: "$50")
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(view.originalAmountLabel.isHidden == false)
        #expect(view.originalAmountLabel.attributedText?.string == "$50")
        #expect(view.subtitleLabel.text == String(localized: .transferStatusAmountDiffers))
        #expect(view.subtitleIconView.isHidden)
    }

    @Test("A short outgoing claim tells the sender how much of the sent amount the peer received")
    func outgoingPartialClaimNamesThePeer() {
        let configuration = makeConfiguration(
            amount: "$2,000,000",
            state: .outgoing(.claimed),
            originalAmountText: "$3,000,000",
            peerName: "Jake"
        )
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(
            view.subtitleLabel.text == String(
                localized: .transferStatusAmountReceivedBy(
                    username: "Jake",
                    received: "$2,000,000",
                    sent: "$3,000,000"
                )
            )
        )
        #expect(view.subtitleLabel.text?.hasPrefix("Jake received $2,000,000 of $3,000,000") == true)
        #expect(view.subtitleLabel.textColor == .fgWarning)
    }

    @Test("A short incoming claim tells the receiver how much of the sent amount arrived")
    func incomingPartialClaimShowsReceivedOfSent() {
        let configuration = makeConfiguration(
            amount: "$2,000,000",
            state: .incoming(.claimed),
            originalAmountText: "$3,000,000"
        )
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(
            view.subtitleLabel.text == String(
                localized: .transferStatusAmountReceived(received: "$2,000,000", sent: "$3,000,000")
            )
        )
        #expect(view.subtitleLabel.text?.hasPrefix("You received $2,000,000 of $3,000,000") == true)
        #expect(view.subtitleLabel.textColor == .fgWarning)
    }

    @Test("A claimed outgoing transfer names the peer who received it")
    func claimedOutgoingNamesThePeer() {
        let configuration = makeConfiguration(state: .outgoing(.claimed), peerName: "Jake")
        let view = ChatTransferMessageView(configuration: configuration)

        #expect(view.subtitleLabel.text == String(localized: .transferStatusReceivedBy(username: "Jake")))
        #expect(view.subtitleLabel.text == "Received by Jake")
    }

    @Test("A failed outgoing transfer reads as not sent, in the warning tint and without an icon")
    func failedOutgoingRendersNotSent() {
        let view = ChatTransferMessageView(configuration: makeConfiguration(state: .outgoing(.failed)))

        #expect(view.subtitleLabel.text == String(localized: .transferStatusNotSent))
        #expect(view.subtitleLabel.textColor == .fgWarning)
        #expect(view.subtitleIconView.isHidden)
    }

    @Test("A failed incoming transfer renders its status in the warning tint without an icon")
    func failedRendersErrorTint() {
        let view = ChatTransferMessageView(configuration: makeConfiguration(state: .incoming(.failed)))

        #expect(view.subtitleLabel.text == String(localized: .transferStatusError))
        #expect(view.subtitleLabel.textColor == .fgWarning)
        #expect(view.subtitleIconView.isHidden)
        #expect(view.originalAmountLabel.isHidden)
    }
}

private extension ChatTransferMessageViewTests {
    func makeView(amount: String) -> ChatTransferMessageView {
        ChatTransferMessageView(configuration: makeConfiguration(amount: amount))
    }

    func makeConfiguration(
        amount: String = "$20",
        state: ChatTransferMessageConfiguration.DirectionalState = .outgoing(.sent),
        originalAmountText: String? = nil,
        peerName: String? = nil
    ) -> ChatTransferMessageConfiguration {
        ChatTransferMessageConfiguration(
            title: "You sent",
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
            side: .trailing,
            peerName: peerName
        )
    }
}

private struct FixedTimestampFormatter: TimestampFormatting {
    func string(for _: Date, now _: Date) -> String { "2:33" }
}
