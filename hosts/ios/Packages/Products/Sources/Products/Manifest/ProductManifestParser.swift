import Foundation
import SDKLogger

/// Product-wide metadata from the root manifest. `icon` is nil when the manifest declares one the
/// Host cannot render — the product stays launchable and shows a placeholder.
struct RootManifest {
    let displayName: String
    let description: String
    let icon: ProductIcon?
}

protocol ProductManifestParsing {
    func parseRoot(_ rawText: String?) -> RootManifest?
    func parseExecutable(
        _ rawText: String?,
        kind: ExecutableKind,
        identifier: ProductId
    ) -> ProductExecutable?
}

/// Every rejection collapses to nil, so a malformed executable is skipped while its siblings load.
struct ProductManifestParser: ProductManifestParsing {
    private enum Constants {
        static let schemaVersion = 1
        static let defaultWidgetWidth = 1
    }

    private let logger: SDKLoggerProtocol
    private let screening: PocketCardScreening

    init(logger: SDKLoggerProtocol, screening: PocketCardScreening) {
        self.logger = logger
        self.screening = screening
    }

    /// An absent record is a legacy product, not a failure.
    func parseRoot(_ rawText: String?) -> RootManifest? {
        guard let rawText, !rawText.isEmpty else { return nil }

        guard let dto: RootManifestDTO = decode(rawText) else {
            return reject("root: malformed JSON")
        }

        guard dto.version == Constants.schemaVersion else {
            return reject("root: unsupported $v=\(String(describing: dto.version))")
        }

        guard let displayName = dto.displayName, !displayName.isEmpty else {
            return reject("root: missing displayName")
        }

        // Unused here, but required by the manifest format: a stricter Host must not see a
        // different product than this one does.
        guard let description = dto.description else {
            return reject("root: missing description")
        }

        // An absent icon fails the schema, so the whole manifest is malformed; only a format the
        // Host cannot render degrades to a placeholder while the product stays launchable.
        guard let iconDTO = dto.icon else {
            return reject("root: missing icon")
        }

        guard let cid = iconDTO.cid, !cid.isEmpty else {
            return reject("root: icon missing cid")
        }

        guard let rawFormat = iconDTO.format else {
            return reject("root: icon missing format")
        }

        let icon = ProductIcon.Format(rawValue: rawFormat.lowercased())
            .map { ProductIcon(cid: cid, format: $0) }
            ?? reject("root: unsupported icon format '\(rawFormat)' — falling back to a placeholder")

        return RootManifest(displayName: displayName, description: description, icon: icon)
    }

    /// `identifier` is the subname the record was read from, and the name its archive resolves under.
    func parseExecutable(
        _ rawText: String?,
        kind: ExecutableKind,
        identifier: ProductId
    ) -> ProductExecutable? {
        guard let rawText, !rawText.isEmpty else { return nil }

        guard let dto: ExecutableManifestDTO = decode(rawText) else {
            return reject("\(identifier): malformed JSON")
        }

        guard dto.version == Constants.schemaVersion else {
            return reject("\(identifier): unsupported $v=\(String(describing: dto.version))")
        }

        guard dto.kind == kind.rawValue else {
            return reject("\(identifier): kind '\(dto.kind ?? "none")' does not match subname '\(kind.rawValue)'")
        }

        guard let appVersion = dto.appVersion?.value else {
            return reject("\(identifier): missing or malformed appVersion")
        }

        return switch kind {
        case .app:
            .app(.init(identifier: identifier, appVersion: appVersion))
        case .widget:
            parseWidget(dto, identifier: identifier, appVersion: appVersion)
        case .worker:
            parseWorker(dto, identifier: identifier, appVersion: appVersion)
        }
    }
}

private extension ProductManifestParser {
    func decode<T: Decodable>(_ rawText: String) -> T? {
        guard let data = rawText.data(using: .utf8) else { return nil }

        return try? JSONDecoder().decode(T.self, from: data)
    }

    // Logs first: a silent nil makes publisher-side bugs invisible.
    func reject<T>(_ reason: String) -> T? {
        logger.warning("Manifest rejected — \(reason)")
        return nil
    }

    func parseWidget(
        _ dto: ExecutableManifestDTO,
        identifier: ProductId,
        appVersion: SemVer
    ) -> ProductExecutable? {
        guard let heights = dto.dimensions?.height, !heights.isEmpty else {
            return reject("\(identifier): widget dimensions.height must list at least one height")
        }

        return .widget(
            .init(
                identifier: identifier,
                appVersion: appVersion,
                description: dto.description,
                heights: heights,
                width: dto.dimensions?.width ?? Constants.defaultWidgetWidth
            )
        )
    }

    func parseWorker(
        _ dto: ExecutableManifestDTO,
        identifier: ProductId,
        appVersion: SemVer
    ) -> ProductExecutable? {
        guard let entrypoint = dto.entrypoint, !entrypoint.isEmpty else {
            return reject("\(identifier): worker missing entrypoint")
        }

        // Declaring neither is valid — the worker runs as background logic with no user-facing
        // surface — but an absent flag means the publisher never declared one.
        guard let includesChat = dto.includes?.chat else {
            return reject("\(identifier): worker includes missing 'chat'")
        }

        guard let includesPocket = dto.includes?.pocket else {
            return reject("\(identifier): worker includes missing 'pocket'")
        }

        // Read whatever the flag says, so cards published without the flag are
        // still reported as the defect they are.
        let cards = pocketCards(dto.pocket, includesPocket: includesPocket, identifier: identifier)
        var modalities: [ProductExecutable.Worker.Modality] = []
        if includesChat { modalities.append(.chat) }
        if includesPocket { modalities.append(.pocket(cards)) }

        return .worker(
            .init(
                identifier: identifier,
                appVersion: appVersion,
                entrypoint: entrypoint,
                modalities: modalities
            )
        )
    }

    /// A stricter Host must not see a different manifest, so cards are read only behind
    /// `includes.pocket` and screened by the core itself. A defect costs the product
    /// its cards and nothing more: failing the worker over one would take its chat with it, and chat
    /// has nothing to do with the cards.
    func pocketCards(
        _ dto: PocketDTO?,
        includesPocket: Bool,
        identifier: ProductId
    ) -> [PocketCardDefinition] {
        let cards: [PocketCardDTO]

        switch dto?.cards ?? .absent {
        case .absent:
            return []
        case .malformed:
            return noCards("\(identifier): pocket section is not the shape the manifest format describes")
        case let .declared(declared):
            cards = declared
        }

        guard includesPocket else {
            return noCards("\(identifier): pocket.cards published without includes.pocket")
        }

        do {
            let definitions = try cards.map { try definition($0) }
            var seen = Set<PocketCardId>()
            guard definitions.allSatisfy({ seen.insert($0.id).inserted }) else {
                return noCards("\(identifier): pocket.cards ids must be unique")
            }
            return definitions
        } catch {
            return noCards("\(identifier): publishing no cards, \(error)")
        }
    }

    func noCards(_ reason: String) -> [PocketCardDefinition] {
        reject(reason) ?? []
    }

    /// The preview is always an archive path: reading it as anything the product spells would let
    /// any product on chain name an address the Host then fetches.
    func definition(_ dto: PocketCardDTO) throws -> PocketCardDefinition {
        guard let rawId = dto.id else { throw PocketCardManifestError.missingField("id") }
        guard let title = dto.title, !title.trimmed.isEmpty else {
            throw PocketCardManifestError.missingField("title")
        }
        guard let preview = dto.preview, !preview.trimmed.isEmpty else {
            throw PocketCardManifestError.missingField("preview")
        }

        return try PocketCardDefinition(
            id: screening.id(rawId),
            title: screening.title(title),
            preview: .archive(path: preview)
        )
    }
}

private enum PocketCardManifestError: Error, CustomStringConvertible {
    case missingField(String)

    var description: String {
        switch self {
        case let .missingField(key): "a pocket card is missing '\(key)'"
        }
    }
}

private extension String {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
