package io.paritytech.polkadotapp.app.root.domain.debug

private val GITHUB_LOGIN = Regex("[A-Za-z0-9]+(-[A-Za-z0-9]+)*")

/** GitHub login a reporter gave, posted unescaped as a mention so GitHub notifies them. */
@JvmInline
value class GitHubUsername private constructor(val value: String) {
    companion object {
        /**
         * Parses what a reporter typed, ignoring surrounding whitespace and one leading `@`.
         *
         * Returns null unless the rest is a GitHub login: at most 39 letters, digits and single hyphens, with no
         * hyphen at either end.
         */
        fun parse(input: String): GitHubUsername? = input.trim().removePrefix("@")
            .takeIf { it.length <= 39 && GITHUB_LOGIN.matches(it) }
            ?.let(::GitHubUsername)
    }
}
