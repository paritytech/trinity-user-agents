import Foundation

// Wire types for the product manifest format (v1). Every field is optional and validated by
// ``ProductManifestParser``: these cross the dotNS trust boundary, and one absent key would
// otherwise abort the whole decode and take an executable's siblings down with it.

struct RootManifestDTO: Decodable {
    let version: Int?
    let displayName: String?
    let description: String?
    let icon: IconDTO?

    enum CodingKeys: String, CodingKey {
        case version = "$v"
        case displayName
        case description
        case icon
    }
}

struct IconDTO: Decodable {
    let cid: String?
    let format: String?
}

/// One type for every kind: the expected kind is known from the subname, so the parser reads the
/// fields that kind needs and ignores the rest.
struct ExecutableManifestDTO: Decodable {
    let version: Int?
    let kind: String?
    let appVersion: SemVerDTO?
    let entrypoint: String?
    let includes: IncludesDTO?
    let pocket: PocketDTO?
    let description: String?
    let dimensions: DimensionsDTO?

    enum CodingKeys: String, CodingKey {
        case version = "$v"
        case kind
        case appVersion
        case entrypoint
        case includes
        case pocket
        case description
        case dimensions
    }
}

struct IncludesDTO: Decodable {
    let chat: Bool?
    let pocket: Bool?
}

/// The section decodes leniently so a publisher who typed it wrong costs the product its cards and
/// not the worker record, which is also where the product's chat is declared.
struct PocketDTO: Decodable {
    /// An absent section and a malformed one are held apart because only the second is worth
    /// reporting: most workers publish no cards at all.
    enum Cards {
        case absent
        case declared([PocketCardDTO])
        case malformed
    }

    let cards: Cards

    private enum CodingKeys: String, CodingKey {
        case cards
    }

    init(from decoder: Decoder) throws {
        do {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            cards = try container.decodeIfPresent([PocketCardDTO].self, forKey: .cards)
                .map(Cards.declared) ?? .absent
        } catch {
            cards = .malformed
        }
    }
}

struct PocketCardDTO: Decodable {
    let id: String?
    let title: String?
    let preview: String?
}

struct DimensionsDTO: Decodable {
    let height: [Int]?
    let width: Int?
}

/// `[major, minor, patch]` or `[major, minor, patch, build]`, where the build element is a string.
struct SemVerDTO: Decodable {
    let value: SemVer?

    init(from decoder: Decoder) throws {
        guard
            var container = try? decoder.unkeyedContainer(),
            let count = container.count, (3 ... 4).contains(count),
            let major = try? container.decode(Int.self),
            let minor = try? container.decode(Int.self),
            let patch = try? container.decode(Int.self)
        else {
            value = nil
            return
        }

        guard count == 4 else {
            value = SemVer(major: major, minor: minor, patch: patch, build: nil)
            return
        }

        value = (try? container.decode(String.self)).map {
            SemVer(major: major, minor: minor, patch: patch, build: $0)
        }
    }
}
