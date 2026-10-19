#if TESTNET_FEATURE
    import Foundation
    import Testing
    import TrUAPIHost
    @testable import polkadot_app

    /// The chat showcase exists so that a tester sees every kind of thing that a product can put in a room. These
    /// tests fail when the wire gains a kind that the showcase does not show, so the showcase grows with the wire.
    /// Each `kind` switch is exhaustive: a new case stops the build here first.
    struct MockChatShowcaseTests {
        private let media = MockChatMedia(
            pictures: Array(repeating: Data([0xFF]), count: 4),
            video: Data([0x00]),
            document: Data([0x25])
        )
        private let now: UInt64 = 1_800_000_000_000

        private var messages: [ChatMessage] {
            MockChatShowcase.showcaseMessages(media: media, now: now)
        }

        @Test func everyBodyKindIsShown() {
            let shown = Set(messages.map { BodyKind($0.body) })

            #expect(shown == Set(BodyKind.allCases))
        }

        @Test func everyStatusIsShown() {
            let shown = Set(messages.map { StatusKind($0.status) })

            #expect(shown == Set(StatusKind.allCases))
        }

        @Test func everyMediaKindIsShown() {
            let items = messages.flatMap { message -> [ChatMediaItem] in
                if case let .richText(_, media) = message.body { media } else { [] }
            }

            #expect(Set(items.map(MediaKind.init)) == Set(MediaKind.allCases))
        }

        @Test func everyParticipantKindIsListed() {
            let listed = Set(MockChatShowcase.participants.map { ParticipantKind($0.participant) })

            #expect(listed == Set(ParticipantKind.allCases))
        }

        /// The room opens on its newest page, so the first page must hold the whole showcase and not only history.
        @Test func theFirstPageOfTheHostHoldsTheWholeShowcase() {
            let conversation = MockChatConversation(messages: messages)
            let firstPage = conversation.page(.before(cursor: nil), size: 50)
            let showcase = messages.filter { !$0.messageId.hasPrefix("m-history") }

            #expect(Set(showcase.map(\.messageId)).isSubset(of: Set(firstPage.messages.map(\.messageId))))
            #expect(firstPage.next != nil)
        }

        @Test func theMetaNamesMessagesOfTheRoom() {
            let ids = Set(messages.map(\.messageId))
            let meta = MockChatShowcase.showcaseMeta(now: now)

            #expect(meta.allSatisfy { ids.contains($0.messageId) })
            #expect(meta.contains { $0.meta.thread != nil })
            #expect(meta.contains { $0.meta.branch != nil })
            #expect(meta.contains { $0.meta.reasoning != nil })
        }

        @Test func aReplyNamesAMessageOfTheRoom() {
            let ids = Set(messages.map(\.messageId))
            let replies = messages.compactMap(\.replyTo)

            #expect(!replies.isEmpty)
            #expect(replies.allSatisfy(ids.contains))
        }

        @Test func theTreesUseEveryNodeTypographyAndColour() {
            let trees = [
                MockChatRenderTrees.card(typed: "", primaryPresses: 0),
                MockChatRenderTrees.counter(0),
                MockChatRenderTrees.footer()
            ]
            var usage = TreeUsage()
            trees.forEach { usage.visit($0) }

            #expect(usage.nodes == Set(NodeKind.allCases))
            #expect(usage.typography == Set(TypographyKind.allCases))
            #expect(usage.colours == Set(ColourKind.allCases))
            #expect(usage.buttons == Set(ButtonKind.allCases))
        }

        @Test func theMenusUseBothKindsAndBothHandlers() {
            let userMessage = messages.first { $0.origin.participantId == Chat.ProductParticipant.user }
            let customMessage = messages.first { if case .custom = $0.body { true } else { false } }
            let textMessage = messages.first { $0.messageId == "m-text" }
            let menus = [userMessage, customMessage, textMessage].compactMap { $0.map(MockChatMenus.messageMenu) }

            let actions = menus.flatMap { menu -> [ChatProductAction] in
                switch menu {
                case let .custom(actions): actions
                case let .extendBuiltIn(actions, _): actions
                }
            }
            #expect(menus.contains { if case .custom = $0 { true } else { false } })
            #expect(menus.contains { if case let .extendBuiltIn(_, hidden) = $0 { !hidden.isEmpty } else { false } })
            #expect(actions.contains { $0.destructive })
            #expect(actions.contains { if case .deeplink = $0.handler { true } else { false } })
        }
    }

    // MARK: - Kinds

    private enum BodyKind: CaseIterable {
        case text
        case markdown
        case richTextWithMedia
        case actionsColumn
        case actionsGrid
        case custom
        case notice

        init(_ body: ChatMessageBody) {
            switch body {
            case .text: self = .text
            case let .richText(_, media): self = media.isEmpty ? .markdown : .richTextWithMedia
            case let .actions(actions): self = actions.layout == .grid ? .actionsGrid : .actionsColumn
            case .custom: self = .custom
            case .notice: self = .notice
            }
        }
    }

    private enum StatusKind: CaseIterable {
        case preparing
        case sent
        case delivered
        case read
        case failed

        init(_ status: ChatMessageStatus) {
            switch status {
            case .preparing: self = .preparing
            case .sent: self = .sent
            case .delivered: self = .delivered
            case .read: self = .read
            case .failed: self = .failed
            }
        }
    }

    private enum MediaKind: CaseIterable {
        case image
        case video
        case file

        init(_ item: ChatMediaItem) {
            switch item {
            case .image: self = .image
            case .video: self = .video
            case .file: self = .file
            }
        }
    }

    private enum ParticipantKind: CaseIterable {
        case person
        case litePerson
        case contact
        case product
        case custom

        init(_ participant: ChatParticipant) {
            switch participant {
            case .person: self = .person
            case .litePerson: self = .litePerson
            case .contact: self = .contact
            case .product: self = .product
            case .custom: self = .custom
            }
        }
    }

    private enum NodeKind: CaseIterable {
        case string
        case box
        case column
        case row
        case spacer
        case text
        case button
        case textField
        case image
        case effect
    }

    private enum TypographyKind: CaseIterable {
        case headlineLarge
        case titleMediumRegular
        case bodyLargeRegular
        case bodyMediumRegular
        case bodySmallRegular

        init(_ style: TypographyStyle) {
            switch style {
            case .headlineLarge: self = .headlineLarge
            case .titleMediumRegular: self = .titleMediumRegular
            case .bodyLargeRegular: self = .bodyLargeRegular
            case .bodyMediumRegular: self = .bodyMediumRegular
            case .bodySmallRegular: self = .bodySmallRegular
            }
        }
    }

    private enum ColourKind: CaseIterable {
        case fgPrimary
        case fgSecondary
        case fgTertiary
        case fgSuccess
        case fgError
        case fgWarning
        case bgSurfaceMain
        case bgSurfaceContainer
        case bgSurfaceNested

        init(_ token: ColorToken) {
            switch token {
            case .fgPrimary: self = .fgPrimary
            case .fgSecondary: self = .fgSecondary
            case .fgTertiary: self = .fgTertiary
            case .fgSuccess: self = .fgSuccess
            case .fgError: self = .fgError
            case .fgWarning: self = .fgWarning
            case .bgSurfaceMain: self = .bgSurfaceMain
            case .bgSurfaceContainer: self = .bgSurfaceContainer
            case .bgSurfaceNested: self = .bgSurfaceNested
            }
        }
    }

    private enum ButtonKind: CaseIterable {
        case primary
        case secondary
        case text

        init(_ variant: ButtonVariant) {
            switch variant {
            case .primary: self = .primary
            case .secondary: self = .secondary
            case .text: self = .text
            }
        }
    }

    /// What a set of trees uses: node kinds, typography, colours of text and backgrounds, and button variants.
    private struct TreeUsage {
        var nodes: Set<NodeKind> = []
        var typography: Set<TypographyKind> = []
        var colours: Set<ColourKind> = []
        var buttons: Set<ButtonKind> = []

        mutating func visit(_ node: RendererNode) {
            switch node {
            case .nil:
                break
            case .string:
                nodes.insert(.string)
            case let .box(modifiers, _, children):
                visit(.box, modifiers: modifiers, children: children)
            case let .column(modifiers, _, children):
                visit(.column, modifiers: modifiers, children: children)
            case let .row(modifiers, _, children):
                visit(.row, modifiers: modifiers, children: children)
            case let .spacer(modifiers):
                visit(.spacer, modifiers: modifiers, children: [])
            case let .text(modifiers, props, children):
                if let style = props.style {
                    typography.insert(TypographyKind(style))
                }
                if let color = props.color {
                    colours.insert(ColourKind(color))
                }
                visit(.text, modifiers: modifiers, children: children)
            case let .button(modifiers, props, children):
                if let variant = props.variant {
                    buttons.insert(ButtonKind(variant))
                }
                visit(.button, modifiers: modifiers, children: children)
            case let .textField(modifiers, _):
                visit(.textField, modifiers: modifiers, children: [])
            case let .image(modifiers, _):
                visit(.image, modifiers: modifiers, children: [])
            case let .effect(_, children):
                visit(.effect, modifiers: [], children: children)
            }
        }

        private mutating func visit(_ kind: NodeKind, modifiers: [Modifier], children: [RendererNode]) {
            nodes.insert(kind)
            for case let .background(background) in modifiers {
                colours.insert(ColourKind(background.color))
            }
            children.forEach { visit($0) }
        }
    }
#endif
