import SwiftUI
import DesignSystem

// MARK: - Action Handler

public typealias WidgetActionHandler = (String, String?) -> Void

private struct WidgetActionHandlerKey: EnvironmentKey {
    static let defaultValue: WidgetActionHandler? = nil
}

/// Resolves where an image node's bytes can be fetched from. A source names a
/// product archive path or a Bulletin CID, which only the host can turn into
/// something loadable, so the renderer asks for it rather than knowing.
///
/// A named type rather than a function typealias, because the resolver is held
/// in the environment and read from a view body. A bare function value carries
/// the isolation of wherever it was formed, which the environment then has to
/// convert away.
public struct WidgetImageResolver: Sendable {
    private let resolve: @Sendable (CustomMessageWidgetNode.ImageSource) async -> URL?

    public init(_ resolve: @escaping @Sendable (CustomMessageWidgetNode.ImageSource) async -> URL?) {
        self.resolve = resolve
    }

    public func callAsFunction(_ source: CustomMessageWidgetNode.ImageSource) async -> URL? {
        await resolve(source)
    }
}

private struct WidgetImageResolverKey: EnvironmentKey {
    static let defaultValue: WidgetImageResolver? = nil
}

extension EnvironmentValues {
    var widgetActionHandler: WidgetActionHandler? {
        get { self[WidgetActionHandlerKey.self] }
        set { self[WidgetActionHandlerKey.self] = newValue }
    }

    var widgetImageResolver: WidgetImageResolver? {
        get { self[WidgetImageResolverKey.self] }
        set { self[WidgetImageResolverKey.self] = newValue }
    }
}

// MARK: - Public Entry Point

public struct CustomMessageWidgetView: View {
    let node: CustomMessageWidgetNode
    var onAction: WidgetActionHandler?
    var resolveImage: WidgetImageResolver?

    public init(
        node: CustomMessageWidgetNode,
        onAction: WidgetActionHandler? = nil,
        resolveImage: WidgetImageResolver? = nil
    ) {
        self.node = node
        self.onAction = onAction
        self.resolveImage = resolveImage
    }

    public var body: some View {
        WidgetNodeContent(node: node)
            .environment(\.widgetActionHandler, onAction)
            .environment(\.widgetImageResolver, resolveImage)
    }
}

// MARK: - Recursive Dispatcher

private struct WidgetNodeContent: View {
    let node: CustomMessageWidgetNode

    var body: some View {
        switch node.content {
        case let .box(props, children):
            NodeBoxView(props: props, children: children, modifiers: node.modifiers)
        case let .column(props, children):
            NodeColumnView(props: props, children: children, modifiers: node.modifiers)
        case let .row(props, children):
            NodeRowView(props: props, children: children, modifiers: node.modifiers)
        case .spacer:
            Spacer(minLength: 0)
                .applyWidgetNodeModifiers(node.modifiers)
        case let .text(props):
            NodeTextView(props: props, modifiers: node.modifiers)
        case let .button(props):
            NodeButtonView(props: props, modifiers: node.modifiers)
        case let .textField(props):
            NodeTextFieldView(props: props, modifiers: node.modifiers)
        case let .image(props):
            NodeImageView(props: props, modifiers: node.modifiers)
        case let .effect(props, children):
            NodeEffectView(props: props, children: children, modifiers: node.modifiers)
        }
    }
}

// MARK: - Image

private struct NodeImageView: View {
    let props: CustomMessageWidgetNode.ImageProps
    let modifiers: CustomMessageWidgetNode.Modifiers

    @Environment(\.widgetImageResolver) private var resolveImage
    @State private var url: URL?

    var body: some View {
        content
            .applyWidgetNodeModifiers(modifiers)
            .task(id: props.source) {
                url = await resolveImage?(props.source)
            }
    }

    /// An image the host cannot fetch draws as empty space, so a missing file
    /// costs the card one node rather than its layout.
    @ViewBuilder
    private var content: some View {
        if let url {
            AsyncImage(url: url) { image in
                props.fit.apply(to: image)
            } placeholder: {
                Color.clear
            }
        } else {
            Color.clear
        }
    }
}

private extension CustomMessageWidgetNode.ImageFit {
    @ViewBuilder
    func apply(to image: Image) -> some View {
        switch self {
        case .none:
            image
        case .fill:
            image.resizable()
        case .cover:
            image.resizable().aspectRatio(contentMode: .fill)
        case .contain:
            image.resizable().aspectRatio(contentMode: .fit)
        case .scaleDown:
            image.resizable().aspectRatio(contentMode: .fit).scaledToFit()
        }
    }
}

// MARK: - Effect

private struct NodeEffectView: View {
    let props: CustomMessageWidgetNode.EffectProps
    let children: [CustomMessageWidgetNode]
    let modifiers: CustomMessageWidgetNode.Modifiers

    var body: some View {
        ZStack {
            ForEach(children.indices, id: \.self) { index in
                WidgetNodeContent(node: children[index])
            }
        }
        .modifier(NodeEffectModifier(effect: props.effect))
        .applyWidgetNodeModifiers(modifiers)
    }
}

private struct NodeEffectModifier: ViewModifier {
    let effect: CustomMessageWidgetNode.Effect

    func body(content: Content) -> some View {
        switch effect {
        case .rainbow:
            content.motionFillingShader(shader: HolographicShaders.iridescentShine)
        }
    }
}

// MARK: - Box

private struct NodeBoxView: View {
    let props: CustomMessageWidgetNode.BoxProps
    let children: [CustomMessageWidgetNode]
    let modifiers: CustomMessageWidgetNode.Modifiers

    var body: some View {
        ZStack(alignment: props.alignment) {
            ForEach(children.indices, id: \.self) { index in
                WidgetNodeContent(node: children[index])
            }
        }
        .applyWidgetNodeModifiers(modifiers, alignment: props.alignment)
    }
}

// MARK: - Column

private struct NodeColumnView: View {
    let props: CustomMessageWidgetNode.ColumnProps
    let children: [CustomMessageWidgetNode]
    let modifiers: CustomMessageWidgetNode.Modifiers

    var body: some View {
        if modifiers.hasWidthConstraint {
            columnContent
                .applyWidgetNodeModifiers(modifiers, alignment: fillAlignment)
        } else {
            ViewThatFits(in: .horizontal) {
                columnContent
                    .fixedSize(horizontal: true, vertical: false)
                    .applyWidgetNodeModifiers(modifiers, alignment: fillAlignment)
                columnContent
                    .applyWidgetNodeModifiers(modifiers, alignment: fillAlignment)
            }
        }
    }

    private var fillAlignment: Alignment {
        Alignment(horizontal: props.alignment, vertical: .top)
    }

    private var columnContent: some View {
        VStack(alignment: props.alignment, spacing: props.arrangement.isSpaced ? 0 : nil) {
            arrangementContent(arrangement: props.arrangement)
        }
    }

    @ViewBuilder
    private func arrangementContent(arrangement: CustomMessageWidgetNode.Arrangement) -> some View {
        switch arrangement {
        case .start:
            childrenContent
        case .end:
            Spacer(minLength: 0)
            childrenContent
        case .center:
            Spacer(minLength: 0)
            childrenContent
            Spacer(minLength: 0)
        case .spaceBetween:
            ForEach(children.indices, id: \.self) { index in
                WidgetNodeContent(node: children[index])
                if index < children.count - 1 {
                    Spacer(minLength: 0)
                }
            }
        case .spaceAround,
             .spaceEvenly:
            ForEach(children.indices, id: \.self) { index in
                if index == 0 { Spacer(minLength: 0) }
                WidgetNodeContent(node: children[index])
                Spacer(minLength: 0)
            }
        }
    }

    @ViewBuilder
    private var childrenContent: some View {
        ForEach(children.indices, id: \.self) { index in
            WidgetNodeContent(node: children[index])
        }
    }
}

// MARK: - Row

private struct NodeRowView: View {
    let props: CustomMessageWidgetNode.RowProps
    let children: [CustomMessageWidgetNode]
    let modifiers: CustomMessageWidgetNode.Modifiers

    var body: some View {
        HStack(alignment: props.alignment, spacing: props.arrangement.isSpaced ? 0 : nil) {
            arrangementContent(arrangement: props.arrangement)
        }
        .fixedSize(
            horizontal: false,
            vertical: !modifiers.hasHeightConstraint
        )
        .applyWidgetNodeModifiers(modifiers, alignment: Alignment(horizontal: .leading, vertical: props.alignment))
    }

    @ViewBuilder
    private func arrangementContent(arrangement: CustomMessageWidgetNode.Arrangement) -> some View {
        switch arrangement {
        case .start:
            childrenContent
        case .end:
            Spacer(minLength: 0)
            childrenContent
        case .center:
            Spacer(minLength: 0)
            childrenContent
            Spacer(minLength: 0)
        case .spaceBetween:
            ForEach(children.indices, id: \.self) { index in
                WidgetNodeContent(node: children[index])
                if index < children.count - 1 {
                    Spacer(minLength: 0)
                }
            }
        case .spaceAround,
             .spaceEvenly:
            ForEach(children.indices, id: \.self) { index in
                if index == 0 { Spacer(minLength: 0) }
                WidgetNodeContent(node: children[index])
                Spacer(minLength: 0)
            }
        }
    }

    @ViewBuilder
    private var childrenContent: some View {
        ForEach(children.indices, id: \.self) { index in
            WidgetNodeContent(node: children[index])
        }
    }
}

// MARK: - Text

private struct NodeTextView: View {
    let props: CustomMessageWidgetNode.TextProps
    let modifiers: CustomMessageWidgetNode.Modifiers

    var body: some View {
        Text(props.text)
            .textStyle(props.labelStyle)
            .foregroundStyle(props.color)
            .applyWidgetNodeModifiers(modifiers)
    }
}

// MARK: - Button

private struct NodeButtonView: View {
    let props: CustomMessageWidgetNode.ButtonProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    @Environment(\.widgetActionHandler) private var actionHandler

    var body: some View {
        LoadableButton(
            isLoading: props.isLoading,
            isEnabled: props.isEnabled
        ) {
            if let actionId = props.clickAction {
                actionHandler?(actionId, nil)
            }
        } label: {
            Text(props.text)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(buttonStyle(for: props.variant))
        .applyWidgetNodeModifiers(modifiers)
    }

    private func buttonStyle(for variant: CustomMessageWidgetNode.ButtonVariant) -> MainButtonStyle {
        switch variant {
        case .primary:
            MainButtonStyle(
                backgroundColor: Color(.textAndIconsPrimaryDark),
                foregroundColor: Color(.textAndIconsPrimaryLight),
                height: 44
            )
        case .secondary:
            MainButtonStyle(
                backgroundColor: Color(.fill6),
                foregroundColor: Color(.textAndIconsPrimaryDark),
                height: 44
            )
        case .text:
            MainButtonStyle(
                backgroundColor: .clear,
                foregroundColor: Color(.textAndIconsPrimaryDark),
                height: 44
            )
        }
    }
}

// MARK: - TextField

private struct NodeTextFieldView: View {
    let props: CustomMessageWidgetNode.TextFieldProps
    let modifiers: CustomMessageWidgetNode.Modifiers
    @Environment(\.widgetActionHandler) private var actionHandler
    @State private var text: String

    init(props: CustomMessageWidgetNode.TextFieldProps, modifiers: CustomMessageWidgetNode.Modifiers) {
        self.props = props
        self.modifiers = modifiers
        _text = State(initialValue: props.text)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let label = props.label {
                Text(label)
                    .typography(.paragraphSmall)
                    .foregroundStyle(Color(.textAndIconsSecondary))
            }

            TextField(
                props.placeholder ?? "",
                text: $text
            )
            .disabled(!props.isEnabled)
            .onChange(of: text) { _, newValue in
                if let actionId = props.valueChangeAction {
                    actionHandler?(actionId, newValue)
                }
            }
        }
        .applyWidgetNodeModifiers(modifiers)
    }
}

// MARK: - Preview

#if DEBUG
    #Preview {
        ScrollView {
            CustomMessageWidgetView(
                node: .previewTransferCard
            ) { actionId, payload in
                print("Action: \(actionId), payload: \(payload ?? "nil")")
            }
            .padding()
        }
    }

    extension CustomMessageWidgetNode {
        // swiftlint:disable function_body_length
        static var previewTransferCard: CustomMessageWidgetNode {
            let title = CustomMessageWidgetNode(
                content: .text(TextProps(
                    text: "Transfer DOT",
                    labelStyle: .title18SemiBold(),
                    color: Color(.textAndIconsPrimaryDark)
                ))
            )

            let subtitle = CustomMessageWidgetNode(
                content: .text(TextProps(
                    text: "Send tokens to another account",
                    labelStyle: .caption12Regular(),
                    color: Color(.textAndIconsSecondary)
                ))
            )

            let textField = CustomMessageWidgetNode(
                content: .textField(TextFieldProps(
                    text: "",
                    placeholder: "Enter address",
                    label: "Recipient",
                    valueChangeAction: "onAddressChange"
                )),
                modifiers: Modifiers(
                    padding: EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0),
                    fillWidth: true
                )
            )

            let primaryButton = CustomMessageWidgetNode(
                content: .button(ButtonProps(
                    text: "Send",
                    variant: .primary,
                    clickAction: "onSend"
                )),
                modifiers: Modifiers(fillWidth: true)
            )

            let secondaryButton = CustomMessageWidgetNode(
                content: .button(ButtonProps(
                    text: "Cancel",
                    variant: .secondary,
                    clickAction: "onCancel"
                )),
                modifiers: Modifiers(fillWidth: true)
            )

            let buttonRow = CustomMessageWidgetNode(
                content: .row(
                    RowProps(alignment: .center, arrangement: .spaceBetween),
                    children: [secondaryButton, primaryButton]
                ),
                modifiers: Modifiers(fillWidth: true)
            )

            let badge = CustomMessageWidgetNode(
                content: .text(TextProps(
                    text: "Connected",
                    labelStyle: .caption10Regular(),
                    color: Color(.textAndIconsPrimaryLight)
                )),
                modifiers: Modifiers(
                    padding: EdgeInsets(top: 4, leading: 8, bottom: 4, trailing: 8),
                    background: Background(
                        color: Color(.brandGreen),
                        shape: AnyShape(RoundedRectangle(cornerRadius: 8))
                    )
                )
            )

            let loadingButton = CustomMessageWidgetNode(
                content: .button(ButtonProps(
                    text: "Processing...",
                    variant: .primary,
                    isLoading: true
                )),
                modifiers: Modifiers(fillWidth: true)
            )

            func spacer(height: CGFloat) -> CustomMessageWidgetNode {
                CustomMessageWidgetNode(
                    content: .spacer,
                    modifiers: Modifiers(height: height)
                )
            }

            return CustomMessageWidgetNode(
                content: .column(
                    ColumnProps(alignment: .leading, arrangement: .start),
                    children: [
                        title,
                        subtitle,
                        spacer(height: 8),
                        badge,
                        spacer(height: 8),
                        textField,
                        spacer(height: 12),
                        buttonRow,
                        spacer(height: 8),
                        loadingButton
                    ]
                ),
                modifiers: Modifiers(
                    padding: EdgeInsets(top: 12, leading: 16, bottom: 12, trailing: 16),
                    background: Background(
                        color: Color(.backgroundSecondary),
                        shape: AnyShape(RoundedRectangle(cornerRadius: 16))
                    ),
                    fillWidth: true
                )
            )
        }

        // swiftlint:enable function_body_length
    }
#endif
