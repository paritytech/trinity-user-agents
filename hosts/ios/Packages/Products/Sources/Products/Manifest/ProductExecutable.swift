import Foundation

/// One deployable artifact. `identifier` is the subname the executable was read from, which is
/// also the name its content archive resolves under.
public enum ProductExecutable: Hashable, Sendable {
    case app(App)
    case widget(Widget)
    case worker(Worker)

    public struct App: Hashable, Sendable {
        public let identifier: ProductId
        public let appVersion: SemVer

        public init(identifier: ProductId, appVersion: SemVer) {
            self.identifier = identifier
            self.appVersion = appVersion
        }
    }

    public struct Widget: Hashable, Sendable {
        public let identifier: ProductId
        public let appVersion: SemVer
        public let description: String?
        /// Grid-step heights the widget can render at.
        public let heights: [Int]
        public let width: Int

        public init(
            identifier: ProductId,
            appVersion: SemVer,
            description: String?,
            heights: [Int],
            width: Int
        ) {
            self.identifier = identifier
            self.appVersion = appVersion
            self.description = description
            self.heights = heights
            self.width = width
        }
    }

    public struct Worker: Hashable, Sendable {
        /// One surface a worker declares, with whatever that surface is
        /// declared with. A worker that declares none is valid: it runs as
        /// background logic with no user-facing surface.
        public enum Modality: Hashable, Sendable {
            case chat
            case pocket([PocketCardDefinition])

            public var kind: ProductWorkerModality {
                switch self {
                case .chat: .chat
                case .pocket: .pocket
                }
            }
        }

        public let identifier: ProductId
        public let appVersion: SemVer
        /// Entry module path relative to the executable directory root.
        public let entrypoint: String
        public let modalities: [Modality]

        public init(
            identifier: ProductId,
            appVersion: SemVer,
            entrypoint: String,
            modalities: [Modality]
        ) {
            self.identifier = identifier
            self.appVersion = appVersion
            self.entrypoint = entrypoint
            self.modalities = modalities
        }

        /// The Pocket cards this worker offers, none unless it serves a Pocket.
        public var pocketCards: [PocketCardDefinition] {
            modalities
                .compactMap { modality in
                    if case let .pocket(cards) = modality { cards } else { nil }
                }
                .first ?? []
        }
    }
}

/// A product's published executables. Any kind can be absent; legacy products carry only an app.
public struct ProductExecutables: Hashable, Sendable {
    public let app: ProductExecutable.App?
    public let widget: ProductExecutable.Widget?
    public let worker: ProductExecutable.Worker?

    public static let empty = ProductExecutables(app: nil, widget: nil, worker: nil)

    public init(
        app: ProductExecutable.App?,
        widget: ProductExecutable.Widget?,
        worker: ProductExecutable.Worker?
    ) {
        self.app = app
        self.widget = widget
        self.worker = worker
    }

    /// Slots the executable into the field its kind owns, replacing whatever was there.
    public func appending(_ executable: ProductExecutable) -> ProductExecutables {
        switch executable {
        case let .app(value):
            ProductExecutables(app: value, widget: widget, worker: worker)
        case let .widget(value):
            ProductExecutables(app: app, widget: value, worker: worker)
        case let .worker(value):
            ProductExecutables(app: app, widget: widget, worker: value)
        }
    }
}
