import UIKit

// TODO: might be a temporary solution to quickly create footer content

/// A content configuration that wraps a UIView for use with `HashableContentConfiguration`.
///
/// This configuration allows you to use any UIView subclass within the diffable content system,
/// similar to how `SwiftUIContentConfiguration` works with SwiftUI views.
///
/// Example usage:
/// ```swift
/// let button = UIButton(type: .system)
/// button.setTitle("Tap Me", for: .normal)
/// let config = UIViewContentConfiguration(
///     id: "myButton",
///     viewProvider: { button }
/// )
/// ```
public struct UIViewContentConfiguration: HashableContentConfiguration {
    /// A unique identifier for this view configuration, used for hashing and equality
    private let id: AnyHashable

    /// A closure that creates or returns the UIView instance
    private let viewProvider: () -> UIView

    /// Creates a UIView content configuration with a unique identifier and view provider.
    ///
    /// - Parameters:
    ///   - id: A unique identifier for this configuration. Used for equality and hashing.
    ///   - viewProvider: A closure that returns the UIView to be displayed.
    ///
    /// - Note: The `viewProvider` closure will be called when `makeContentView()` is invoked.
    ///         If you want to reuse the same view instance, capture it in the closure.
    public init(id: some Hashable, viewProvider: @escaping () -> UIView) {
        self.id = AnyHashable(id)
        self.viewProvider = viewProvider
    }

    /// Convenience initializer that generates a UUID as the identifier.
    ///
    /// - Parameter viewProvider: A closure that returns the UIView to be displayed.
    ///
    /// - Warning: Using this initializer means each configuration will have a unique ID,
    ///            which may cause unnecessary reconfigurations in diffable data sources.
    public init(viewProvider: @escaping () -> UIView) {
        self.init(id: UUID(), viewProvider: viewProvider)
    }

    public func makeContentView() -> UIView & UIContentView {
        UIViewContentView(configuration: self)
    }

    // MARK: - Hashable

    // TODO: remove tmp class
    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.id == rhs.id && lhs.viewProvider() == rhs.viewProvider()
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }

    // MARK: - Internal

    fileprivate func createView() -> UIView {
        viewProvider()
    }
}

// MARK: - Content View

private final class UIViewContentView: UIView, UIContentView {
    private var appliedConfiguration: UIViewContentConfiguration
    private var wrappedView: UIView?

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    init(configuration: UIViewContentConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupWrappedView()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupWrappedView() {
        let view = appliedConfiguration.createView()
        addSubview(view)

        // Use Auto Layout to fill the container
        view.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: topAnchor),
            view.leadingAnchor.constraint(equalTo: leadingAnchor),
            view.trailingAnchor.constraint(equalTo: trailingAnchor),
            view.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        wrappedView = view
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? UIViewContentConfiguration else { return }
        guard appliedConfiguration != configuration else {
            return
        }
        wrappedView?.removeFromSuperview()
        appliedConfiguration = configuration
        setupWrappedView()
    }
}
