#if DEBUG
    import UIKit

    @MainActor
    private enum ChatTransferMessagePreview {
        static func makeView() -> UIView {
            let bubbles = incomingBubbles() + outgoingBubbles()

            let stack = UIStackView(arrangedSubviews: bubbles)
            stack.axis = .vertical
            stack.spacing = 20
            stack.isLayoutMarginsRelativeArrangement = true
            stack.layoutMargins = UIEdgeInsets(top: 20, left: 16, bottom: 20, right: 16)

            let scroll = UIScrollView()
            scroll.backgroundColor = .bgSurfaceMain
            scroll.addSubview(stack)
            stack.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                stack.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
                stack.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
                stack.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
                stack.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
                stack.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor)
            ])
            return scroll
        }

        static func incomingBubbles() -> [UIView] {
            let states: [(ChatTransferMessageConfiguration.IncomingState, String?)] = [
                (.detecting, nil),
                (.claiming, nil),
                (.claimed, nil),
                (.claimed, "55"),
                (.failed, nil)
            ]
            return states.map { state, originalAmount in
                let configuration = ChatTransferMessageConfiguration.inbox(
                    currencySymbol: "$",
                    amount: "17",
                    tokenSymbol: "DOT",
                    originalAmount: originalAmount,
                    from: "Samuel.long.long.18",
                    state: state,
                    statusConfiguration: .init(
                        dateFormatter: TimestampFormatter(),
                        date: .now,
                        textColor: .fgPrimary,
                        image: nil,
                        isEdited: false
                    )
                )
                return configuration.makeContentView()
            }
        }

        static func outgoingBubbles() -> [UIView] {
            let states: [(ChatTransferMessageConfiguration.OutgoingState, String?)] = [
                (.sending, nil),
                (.sent, nil),
                (.claimed, nil),
                (.claimed, "99"),
                (.failed, nil)
            ]
            return states.map { state, originalAmount in
                let configuration = ChatTransferMessageConfiguration.outbox(
                    currencySymbol: "$",
                    amount: originalAmount == nil ? "99" : "40",
                    tokenSymbol: "DOT",
                    originalAmount: originalAmount,
                    state: state,
                    statusConfiguration: .init(
                        dateFormatter: TimestampFormatter(),
                        date: .now,
                        textColor: .fgPrimaryInverted,
                        image: nil,
                        isEdited: false
                    )
                )
                return configuration.makeContentView()
            }
        }
    }

    #Preview("Transfer bubble states") {
        ChatTransferMessagePreview.makeView()
    }
#endif
