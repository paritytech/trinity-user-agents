package io.paritytech.polkadotapp.feature_products_impl.data.manifest

import com.google.gson.Gson
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardDefinition
import io.paritytech.polkadotapp.feature_products_api.model.PocketCardPreview
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductIcon
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private const val CID = "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco"

class ManifestParserTest {
    private val parser = ManifestParser(Gson())

    private fun host(value: String) = ExecutableHost(value)

    private fun rejects(rawText: String) = assertTrue(
        "expected a rejection for $rawText",
        parser.parseRoot(rawText).isFailure,
    )

    private fun rejectsExecutable(rawText: String, kind: ExecutableKind) = assertTrue(
        "expected a rejection for $rawText",
        parser.parseExecutable(rawText, kind, host("${kind.manifestKind}.coinflip.dot")).isFailure,
    )

    /**
     * A card the core would screen out is not published here either, but the worker record still
     * loads: failing it would take the product's chat down over a defect in its Pocket block.
     */
    private fun publishesNoCards(rawText: String) {
        val worker = parser.parseExecutable(rawText, ExecutableKind.WORKER, host("worker.coinflip.dot"))
            .getOrNull() as? ProductExecutable.Worker

        assertEquals("expected a worker with no cards for $rawText", emptyList<PocketCardDefinition>(), worker?.pocketCards)
    }

    @Test
    fun `parses valid root manifest`() {
        val root = parser.parseRoot(
            """{"${'$'}v":1,"displayName":"Coinflip","description":"A game","icon":{"cid":"$CID","format":"png"}}"""
        ).getOrNull()

        assertEquals("Coinflip", root?.displayName)
        assertEquals(CID, root?.icon?.cid?.toString())
        assertEquals(ProductIcon.Format.PNG, root?.icon?.format)
    }

    @Test
    fun `root manifests that a stricter host would reject do not load here either`() {
        // Unknown schema version, malformed JSON, and every RFC-0001 required field in turn.
        rejects("""{"${'$'}v":2,"displayName":"X","description":"d","icon":{"cid":"$CID","format":"png"}}""")
        rejects("not json")
        rejects("")
        rejects("""{"${'$'}v":1,"description":"d","icon":{"cid":"$CID","format":"png"}}""")
        rejects("""{"${'$'}v":1,"displayName":"X","icon":{"cid":"$CID","format":"png"}}""")
        rejects("""{"${'$'}v":1,"displayName":"X","description":"d"}""")
        rejects("""{"${'$'}v":1,"displayName":"X","description":"d","icon":{"format":"png"}}""")
        rejects("""{"${'$'}v":1,"displayName":"X","description":"d","icon":{"cid":"not-a-cid","format":"png"}}""")
    }

    @Test
    fun `root with unsupported icon format stays launchable without an icon`() {
        // RFC-0001: unknown icon format renders a placeholder; the product remains launchable.
        val root = parser.parseRoot(
            """{"${'$'}v":1,"displayName":"Coinflip","description":"d","icon":{"cid":"$CID","format":"gif"}}"""
        ).getOrNull()

        assertEquals("Coinflip", root?.displayName)
        assertNull(root?.icon)
    }

    @Test
    fun `parses app executable`() {
        val app = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"app","appVersion":[1,2,3]}""",
            ExecutableKind.APP,
            host("app.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.App

        assertEquals(host("app.coinflip.dot"), app?.host)
        assertEquals("1.2.3", app?.appVersion.toString())
    }

    @Test
    fun `parses widget executable with dimensions`() {
        val widget = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"widget","appVersion":[0,1,0],"description":"w","dimensions":{"height":[1,2],"width":3}}""",
            ExecutableKind.WIDGET,
            host("widget.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Widget

        assertEquals(listOf(1, 2), widget?.heights)
        assertEquals(3, widget?.width)
        assertEquals("w", widget?.description)
    }

    @Test
    fun `widget without width defaults to one column`() {
        val widget = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"widget","appVersion":[0,1,0],"dimensions":{"height":[2]}}""",
            ExecutableKind.WIDGET,
            host("widget.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Widget

        assertEquals(1, widget?.width)
    }

    @Test
    fun `parses worker executable and formats semver build`() {
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0,"abc"],"entrypoint":"index.js","includes":{"chat":true,"pocket":false}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals("https://worker.coinflip.dot/index.js", worker?.scriptUrl)
        assertEquals("1.0.0+abc", worker?.appVersion.toString())
        assertEquals(true, worker?.includesChat)
        assertEquals(false, worker?.includesPocket)
    }

    @Test
    fun `worker with no included surface is a valid background-only worker`() {
        // RFC: includes { chat: false, pocket: false } is valid — a worker that exposes no
        // user-facing surface and runs purely as background logic.
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":false,"pocket":false}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals(false, worker?.includesChat)
        assertEquals(false, worker?.includesPocket)
    }

    @Test
    fun `worker publishes pocket cards behind includes pocket`() {
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":false,"pocket":true},
               "pocket":{"cards":[{"id":" loyalty ","title":"Loyalty","preview":"faces/loyalty.json"}]}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals(true, worker?.includesPocket)
        val expected = PocketCardDefinition(
            PocketCardId("loyalty"),
            "Loyalty",
            PocketCardPreview.Archive("faces/loyalty.json"),
        )
        assertEquals(listOf(expected), worker?.pocketCards)
    }

    /**
     * A manifest is published on chain by the product, and the preview is read before the user has
     * approved anything. Reading it must stay inside the product's own archive: a manifest that
     * could name a URL would be a manifest that could point the host at any address it liked.
     */
    @Test
    fun `a published card's preview is always a path in the archive, never a url`() {
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":false,"pocket":true},
               "pocket":{"cards":[{"id":"loyalty","title":"Loyalty","preview":"https://example.invalid/face.json"}]}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals(
            PocketCardPreview.Archive("https://example.invalid/face.json"),
            worker?.pocketCards?.single()?.preview,
        )
    }

    @Test
    fun `worker that includes pocket but publishes no cards is valid and has none`() {
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":false,"pocket":true}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals(emptyList<PocketCardDefinition>(), worker?.pocketCards)
    }

    @Test
    fun `pocket cards that a stricter host would reject publish no cards here either`() {
        fun worker(pocket: String, includesPocket: Boolean = true) =
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js","includes":{"chat":false,"pocket":$includesPocket},"pocket":$pocket}"""

        // Cards without the include, a bad id (screened as a chat identifier), duplicate ids, and missing fields.
        publishesNoCards(worker("""{"cards":[{"id":"a","title":"A","preview":"a.json"}]}""", includesPocket = false))
        publishesNoCards(worker("""{"cards":[{"id":"loy\u200dalty","title":"A","preview":"a.json"}]}"""))
        publishesNoCards(worker("""{"cards":[{"id":"a","title":"A","preview":"a.json"},{"id":"a","title":"B","preview":"b.json"}]}"""))
        publishesNoCards(worker("""{"cards":[{"id":"a","preview":"a.json"}]}"""))
        publishesNoCards(worker("""{"cards":[{"id":"a","title":"A"}]}"""))
        publishesNoCards(worker("""{"cards":[{"id":"a","title":"","preview":"a.json"}]}"""))
        publishesNoCards(worker("""{}"""))
    }

    /**
     * A title is drawn, not matched, so it carries the core's display rules, the ones iOS applies too:
     * a title that could reorder the text around it is refused, while the variation selector an emoji
     * needs is kept. Screening it as an id would cost a product every card it publishes.
     */
    @Test
    fun `a card title is screened by the core's display rules`() {
        fun worker(title: String) =
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js","includes":{"chat":false,"pocket":true},
               "pocket":{"cards":[{"id":"coffee","title":"$title","preview":"a.json"}]}}"""

        publishesNoCards(worker("Coffee\\u202e"))
        // NEXT LINE is not blank to Kotlin, but the core trims it away and would leave the card untitled.
        publishesNoCards(worker("\\u0085"))

        val worker = parser.parseExecutable(worker(" \\u2615\\ufe0f Coffee "), ExecutableKind.WORKER, host("worker.coinflip.dot"))
            .getOrNull() as? ProductExecutable.Worker
        assertEquals("☕️ Coffee", worker?.pocketCards?.single()?.title)
    }

    @Test
    fun `executables that do not conform are rejected`() {
        // Incomplete includes: RFC-0001 types it as Record<'chat' | 'pocket', boolean>.
        rejectsExecutable("""{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js","includes":{"chat":true}}""", ExecutableKind.WORKER)
        // A record read from worker.<base> that declares kind=app must not load as a worker.
        rejectsExecutable("""{"${'$'}v":1,"kind":"app","appVersion":[1,0,0]}""", ExecutableKind.WORKER)
        rejectsExecutable("""{"${'$'}v":2,"kind":"app","appVersion":[1,0,0]}""", ExecutableKind.APP)
        rejectsExecutable("""{"${'$'}v":1,"kind":"app","appVersion":[1,2]}""", ExecutableKind.APP)
        rejectsExecutable("""{"${'$'}v":1,"kind":"app","appVersion":[1,2,3,4,5]}""", ExecutableKind.APP)
        rejectsExecutable("""{"${'$'}v":1,"kind":"app"}""", ExecutableKind.APP)
        rejectsExecutable("""{"${'$'}v":1,"kind":"widget","appVersion":[1,0,0],"dimensions":{"height":[]}}""", ExecutableKind.WIDGET)
    }

    // The worker record carries chat and Pocket together. A product that publishes a bad card must
    // not lose the chat it was already serving.
    @Test
    fun `a worker with a malformed pocket block still serves its other modalities`() {
        val worker = parser.parseExecutable(
            """{"${'$'}v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"i.js","includes":{"chat":true,"pocket":true},
               "pocket":{"cards":[{"id":"a","title":"","preview":"a.json"}]}}""",
            ExecutableKind.WORKER,
            host("worker.coinflip.dot"),
        ).getOrNull() as? ProductExecutable.Worker

        assertEquals(true, worker?.includesChat)
        assertEquals(emptyList<PocketCardDefinition>(), worker?.pocketCards)
    }
}
