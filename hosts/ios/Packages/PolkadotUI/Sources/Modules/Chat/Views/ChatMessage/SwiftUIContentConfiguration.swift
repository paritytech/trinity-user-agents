import SwiftUI

public struct SwiftUIContentConfiguration: HashableContentConfiguration {
    let provider: any ContentConfigurationProviding

    public init(
        view: some View & Hashable,
        id: AnyHashable? = nil,
        margins: EdgeInsets = EdgeInsets()
    ) {
        provider = SwiftUIContentConfigurationProvider(view: view, id: id, margins: margins)
    }

    public func makeContentView() -> any UIView & UIContentView {
        SwiftUIContentView(
            configuration: provider.configuration()
        )
    }

    public static func == (lhs: SwiftUIContentConfiguration, rhs: SwiftUIContentConfiguration) -> Bool {
        lhs.provider.equalTo(rhs.provider)
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(provider)
    }
}

private final class SwiftUIContentView: UIView, UIContentView {
    var swiftUIContentView: UIView & UIContentView

    var configuration: any UIContentConfiguration {
        get {
            swiftUIContentView.configuration
        }
        set {
            guard let swiftUIConfiguration = newValue as? SwiftUIContentConfiguration else {
                recreateContentView(configuration: newValue)
                assertionFailure("not supported configuration")
                return
            }
            let newConfiguration = swiftUIConfiguration.provider.configuration()
            if swiftUIContentView.supports(newConfiguration) {
                swiftUIContentView.configuration = newConfiguration
                swiftUIContentView.invalidateIntrinsicContentSize()
            } else {
                recreateContentView(configuration: newConfiguration)
            }
        }
    }

    func supports(_ configuration: any UIContentConfiguration) -> Bool {
        configuration is SwiftUIContentConfiguration
    }

    init(configuration: any UIContentConfiguration) {
        swiftUIContentView = configuration.makeContentView()
        super.init(frame: .zero)

        fillWithContent()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func recreateContentView(configuration: UIContentConfiguration) {
        swiftUIContentView.removeFromSuperview()
        swiftUIContentView = configuration.makeContentView()
        fillWithContent()
    }

    private func fillWithContent() {
        addSubview(swiftUIContentView)
        swiftUIContentView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
    }
}

private class SwiftUIContentConfigurationProvider<Content: View & Hashable>: ContentConfigurationProviding {
    let view: Content
    let id: AnyHashable?
    let margins: EdgeInsets

    lazy var config: UIContentConfiguration = createConfiguration()

    init(view: Content, id: AnyHashable? = nil, margins: EdgeInsets = EdgeInsets()) {
        self.view = view
        self.id = id
        self.margins = margins
    }

    func configuration() -> UIContentConfiguration {
        config
    }

    static func == (lhs: SwiftUIContentConfigurationProvider, rhs: SwiftUIContentConfigurationProvider) -> Bool {
        lhs.id == rhs.id &&
            lhs.view == rhs.view &&
            lhs.margins == rhs.margins
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
        hasher.combine(view)
        hasher.combine(margins.top)
        hasher.combine(margins.leading)
        hasher.combine(margins.bottom)
        hasher.combine(margins.trailing)
    }

    func createConfiguration() -> any UIContentConfiguration {
        guard let id else {
            return UIHostingConfiguration { view }.margins(.all, margins)
        }
        return UIHostingConfiguration { view.id(id) }.margins(.all, margins)
    }
}
