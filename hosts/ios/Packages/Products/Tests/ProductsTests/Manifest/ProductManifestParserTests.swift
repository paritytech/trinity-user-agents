import Foundation
import Testing
@testable import Products

/// Cases mirror the conformance fixtures the manifest specification lists for Hosts.
struct ProductManifestParserTests {
    private let parser = ProductManifestParser(logger: SilentLogger(), screening: .passingThrough)

    // MARK: - Root manifest

    @Test func parsesWellFormedRootManifest() throws {
        let root = try #require(parser.parseRoot(Fixtures.root()))

        #expect(root.displayName == "HackM3")
        #expect(root.description == "A hackathon product")
        #expect(root.icon?.cid == "bafyicon")
        #expect(root.icon?.format == .png)
    }

    @Test func treatsAbsentRootRecordAsLegacyRatherThanFailure() {
        #expect(parser.parseRoot(nil) == nil)
        #expect(parser.parseRoot("") == nil)
    }

    @Test func rejectsMalformedRootJson() {
        #expect(parser.parseRoot("{not json") == nil)
    }

    @Test func rejectsUnknownSchemaVersion() {
        #expect(parser.parseRoot(Fixtures.root(version: 2)) == nil)
    }

    @Test func rejectsRootMissingRequiredFields() {
        #expect(parser.parseRoot(#"{"$v":1,"description":"d","icon":{"cid":"c","format":"png"}}"#) == nil)
        #expect(parser.parseRoot(#"{"$v":1,"displayName":"n","icon":{"cid":"c","format":"png"}}"#) == nil)
    }

    /// An unrenderable icon costs the icon, not the product.
    @Test func keepsProductLaunchableWhenIconFormatIsUnknown() throws {
        let root = try #require(parser.parseRoot(Fixtures.root(iconFormat: "webp")))

        #expect(root.displayName == "HackM3")
        #expect(root.icon == nil)
    }

    @Test func rejectsRootWithoutIcon() {
        #expect(parser.parseRoot(#"{"$v":1,"displayName":"n","description":"d"}"#) == nil)
    }

    @Test func acceptsUppercaseIconFormat() throws {
        let root = try #require(parser.parseRoot(Fixtures.root(iconFormat: "PNG")))

        #expect(root.icon?.format == .png)
    }

    // MARK: - Executable manifests

    @Test func parsesAppManifest() throws {
        let executable = parser.parseExecutable(
            Fixtures.app(),
            kind: .app,
            identifier: "app.hackm3.dot"
        )

        guard case let .app(app)? = executable else {
            Issue.record("expected an app executable, got \(String(describing: executable))")
            return
        }

        #expect(app.identifier == "app.hackm3.dot")
        #expect(app.appVersion == SemVer(major: 1, minor: 2, patch: 3, build: nil))
    }

    @Test func parsesBuildIdentifierFromFourElementVersion() throws {
        let executable = parser.parseExecutable(
            Fixtures.app(version: #"[1, 2, 3, "abc123"]"#),
            kind: .app,
            identifier: "app.hackm3.dot"
        )

        guard case let .app(app)? = executable else {
            Issue.record("expected an app executable")
            return
        }

        #expect(app.appVersion == SemVer(major: 1, minor: 2, patch: 3, build: "abc123"))
    }

    @Test(arguments: [
        "[1, 2]",
        "[1, 2, 3, 4]",
        #"[1, 2, 3, "a", "b"]"#,
        #""1.2.3""#
    ])
    func rejectsMalformedVersionTuples(_ version: String) {
        let executable = parser.parseExecutable(
            Fixtures.app(version: version),
            kind: .app,
            identifier: "app.hackm3.dot"
        )

        #expect(executable == nil)
    }

    /// A kind that disagrees with the subname is malformed; it is never coerced to the label.
    @Test func rejectsKindThatDoesNotMatchTheSubname() {
        let executable = parser.parseExecutable(
            Fixtures.app(),
            kind: .worker,
            identifier: "worker.hackm3.dot"
        )

        #expect(executable == nil)
    }

    @Test func rejectsUnknownKind() {
        let raw = #"{"$v":1,"kind":"gadget","appVersion":[1,0,0]}"#

        #expect(parser.parseExecutable(raw, kind: .app, identifier: "app.hackm3.dot") == nil)
    }

    @Test func treatsAbsentExecutableRecordAsNotProvided() {
        #expect(parser.parseExecutable(nil, kind: .app, identifier: "app.hackm3.dot") == nil)
        #expect(parser.parseExecutable("", kind: .app, identifier: "app.hackm3.dot") == nil)
    }

    @Test func parsesWidgetManifest() throws {
        let executable = parser.parseExecutable(
            Fixtures.widget(),
            kind: .widget,
            identifier: "widget.hackm3.dot"
        )

        guard case let .widget(widget)? = executable else {
            Issue.record("expected a widget executable")
            return
        }

        #expect(widget.heights == [2, 4])
        #expect(widget.width == 3)
        #expect(widget.description == "A tagline")
    }

    @Test func defaultsWidgetWidthToOneColumn() throws {
        let executable = parser.parseExecutable(
            Fixtures.widget(width: nil),
            kind: .widget,
            identifier: "widget.hackm3.dot"
        )

        guard case let .widget(widget)? = executable else {
            Issue.record("expected a widget executable")
            return
        }

        #expect(widget.width == 1)
    }

    @Test func rejectsWidgetWithoutHeights() {
        #expect(parser.parseExecutable(Fixtures.widget(heights: "[]"), kind: .widget, identifier: "w") == nil)
        #expect(parser.parseExecutable(
            #"{"$v":1,"kind":"widget","appVersion":[1,0,0]}"#,
            kind: .widget,
            identifier: "w"
        ) == nil)
    }

    @Test func parsesWorkerManifest() throws {
        let executable = parser.parseExecutable(
            Fixtures.worker(),
            kind: .worker,
            identifier: "worker.hackm3.dot"
        )

        guard case let .worker(worker)? = executable else {
            Issue.record("expected a worker executable")
            return
        }

        #expect(worker.entrypoint == "src/worker.js")
        #expect(worker.serves(.chat))
        #expect(!worker.serves(.pocket))
    }

    /// A worker serving no user-facing surface is valid and still launches.
    @Test func acceptsWorkerServingNoSurface() throws {
        let executable = parser.parseExecutable(
            Fixtures.worker(chat: "false", pocket: "false"),
            kind: .worker,
            identifier: "worker.hackm3.dot"
        )

        guard case let .worker(worker)? = executable else {
            Issue.record("expected a worker executable")
            return
        }

        #expect(!worker.serves(.chat))
        #expect(!worker.serves(.pocket))
    }

    // MARK: - Pocket cards

    @Test func parsesPocketCardsAWorkerPublishes() throws {
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", cards: Fixtures.cards())))

        #expect(worker.pocketCards.map(\.id.value) == ["loyalty", "trophy"])
        #expect(worker.pocketCards.map(\.title) == ["Loyalty", "Trophy"])
        #expect(worker.pocketCards.first?.preview == .archive(path: "faces/loyalty.json"))
    }

    /// A published manifest must never make the Host fetch an address of the
    /// product's choosing, so a preview is read as an archive path whatever it
    /// spells.
    @Test func readsAPreviewAsAnArchivePathEvenWhenItSpellsAUrl() throws {
        let cards = #"[{"id":"loyalty","title":"Loyalty","preview":"https://evil.example/face.json"}]"#
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", cards: cards)))

        #expect(worker.pocketCards.first?.preview == .archive(path: "https://evil.example/face.json"))
    }

    /// A stricter Host must not see a different manifest than this one, so cards
    /// are read only behind the flag that declares them.
    @Test func publishesNoCardsWithoutThePocketInclude() throws {
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "false", cards: Fixtures.cards())))

        #expect(worker.pocketCards.isEmpty)
    }

    @Test func publishesNoCardsWhenTheWorkerDeclaresNone() throws {
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true")))

        #expect(worker.pocketCards.isEmpty)
    }

    /// A defect in the cards costs the product its cards and nothing more:
    /// failing the worker record over one would take the product's chat with it.
    @Test func keepsTheWorkerWhenACardIsMalformed() throws {
        let missingTitle = #"[{"id":"loyalty","preview":"faces/loyalty.json"}]"#
        let blankPreview = #"[{"id":"loyalty","title":"Loyalty","preview":"  "}]"#

        for cards in [missingTitle, blankPreview] {
            let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", cards: cards)))

            #expect(worker.serves(.chat))
            #expect(worker.pocketCards.isEmpty)
        }
    }

    /// A pocket section whose JSON types are wrong is the same defect as a card
    /// the parser refuses, so it costs the same: the cards. Decoding it as part
    /// of the executable record would instead lose the worker, and with it the
    /// chat the product serves from the very same record.
    @Test(arguments: [
        #"{"cards":{"loyalty":{"title":"Loyalty","preview":"faces/loyalty.json"}}}"#,
        #"{"cards":[{"id":7,"title":"Loyalty","preview":"faces/loyalty.json"}]}"#,
        #"{"cards":"faces/loyalty.json"}"#,
        #""cards""#,
        "[]"
    ])
    func keepsTheWorkerWhenThePocketSectionHasTheWrongJsonTypes(_ section: String) throws {
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", pocketSection: section)))

        #expect(worker.entrypoint == "src/worker.js")
        #expect(worker.serves(.chat))
        #expect(worker.pocketCards.isEmpty)
    }

    /// Two cards under one id would make the card a product hands out ambiguous,
    /// so the whole set is refused rather than one of them picked.
    @Test func publishesNoCardsWhenIdsRepeat() throws {
        let cards = """
        [{"id":"loyalty","title":"One","preview":"a.json"},
         {"id":"loyalty","title":"Two","preview":"b.json"}]
        """
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", cards: cards)))

        #expect(worker.pocketCards.isEmpty)
    }

    /// The two rules are not one rule, and the core draws the line: an id is
    /// addressed, a title is only drawn. A parser that screened both alike
    /// would cost a product every card over a legitimate emoji in a title.
    @Test func screensAnIdAndATitleThroughTheirOwnRules() throws {
        let screening = PocketCardScreening(
            id: { raw in
                guard raw == "loyalty" else { throw ScreeningRefusal.refused }
                return raw
            },
            title: { raw in
                guard raw == "Loyalty" else { throw ScreeningRefusal.refused }
                return raw
            }
        )
        let parser = ProductManifestParser(logger: SilentLogger(), screening: screening)

        let swapped = #"[{"id":"Loyalty","title":"loyalty","preview":"faces/loyalty.json"}]"#
        #expect(parsedWorker(Fixtures.worker(pocket: "true", cards: swapped), parser: parser)?.pocketCards
            .isEmpty == true)

        let correct = #"[{"id":"loyalty","title":"Loyalty","preview":"faces/loyalty.json"}]"#
        #expect(parsedWorker(Fixtures.worker(pocket: "true", cards: correct), parser: parser)?.pocketCards.count == 1)
    }

    /// The screened value is what the card is stored and addressed under, not
    /// the raw text, so a host comparing against the core's form still matches.
    @Test func keepsWhatTheScreeningReturnedRatherThanTheRawText() throws {
        let screening = PocketCardScreening(id: { _ in "screened-id" }, title: { _ in "Screened Title" })
        let parser = ProductManifestParser(logger: SilentLogger(), screening: screening)

        let cards = #"[{"id":"  raw  ","title":"  raw  ","preview":"faces/loyalty.json"}]"#
        let worker = try #require(parsedWorker(Fixtures.worker(pocket: "true", cards: cards), parser: parser))

        #expect(worker.pocketCards.map(\.id.value) == ["screened-id"])
        #expect(worker.pocketCards.map(\.title) == ["Screened Title"])
    }

    private func parsedWorker(
        _ manifest: String,
        parser: ProductManifestParser? = nil
    ) -> ProductExecutable.Worker? {
        guard case let .worker(worker)? = (parser ?? self.parser).parseExecutable(
            manifest,
            kind: .worker,
            identifier: "worker.hackm3.dot"
        ) else {
            Issue.record("expected a worker executable")
            return nil
        }
        return worker
    }

    @Test func rejectsWorkerMissingEntrypointOrIncludes() {
        #expect(parser.parseExecutable(
            #"{"$v":1,"kind":"worker","appVersion":[1,0,0],"includes":{"chat":true,"pocket":false}}"#,
            kind: .worker,
            identifier: "w"
        ) == nil)

        #expect(parser.parseExecutable(
            #"{"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js"}"#,
            kind: .worker,
            identifier: "w"
        ) == nil)

        #expect(parser.parseExecutable(
            #"{"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js","includes":{"chat":true}}"#,
            kind: .worker,
            identifier: "w"
        ) == nil)
    }
}

private enum Fixtures {
    static func root(version: Int = 1, iconFormat: String = "png") -> String {
        """
        {"$v":\(version),"displayName":"HackM3","description":"A hackathon product",
         "icon":{"cid":"bafyicon","format":"\(iconFormat)"}}
        """
    }

    static func app(version: String = "[1, 2, 3]") -> String {
        #"{"$v":1,"kind":"app","appVersion":\#(version)}"#
    }

    static func widget(heights: String = "[2, 4]", width: Int? = 3) -> String {
        let widthField = width.map { ",\"width\":\($0)" } ?? ""
        return """
        {"$v":1,"kind":"widget","appVersion":[1,0,0],"description":"A tagline",
         "dimensions":{"height":\(heights)\(widthField)}}
        """
    }

    static func worker(chat: String = "true", pocket: String = "false", cards: String? = nil) -> String {
        worker(chat: chat, pocket: pocket, pocketSection: cards.map { "{\"cards\":\($0)}" })
    }

    static func worker(chat: String = "true", pocket: String = "false", pocketSection: String?) -> String {
        let pocketField = pocketSection.map { ",\"pocket\":\($0)" } ?? ""
        return """
        {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"src/worker.js",
         "includes":{"chat":\(chat),"pocket":\(pocket)}\(pocketField)}
        """
    }

    static func cards() -> String {
        """
        [{"id":"loyalty","title":"Loyalty","preview":"faces/loyalty.json"},
         {"id":"trophy","title":"Trophy","preview":"faces/trophy.json"}]
        """
    }
}

private enum ScreeningRefusal: Error {
    case refused
}
