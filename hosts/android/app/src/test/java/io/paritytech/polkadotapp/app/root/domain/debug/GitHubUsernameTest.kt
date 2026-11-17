package io.paritytech.polkadotapp.app.root.domain.debug

import org.junit.Assert.assertEquals
import org.junit.Test

class GitHubUsernameTest {
    @Test
    fun `only GitHub logins are accepted because the name is posted unescaped as a mention`() {
        val expected = mapOf(
            " @octo-cat " to "octo-cat",
            "a".repeat(39) to "a".repeat(39),
            "a".repeat(40) to null,
            "-octocat" to null,
            "octocat-" to null,
            "octo--cat" to null,
            "octo_cat" to null,
            "octocat\n## Logs" to null,
            "@@octocat" to null,
            "" to null,
        )

        assertEquals(expected, expected.keys.associateWith { GitHubUsername.parse(it)?.value })
    }
}
