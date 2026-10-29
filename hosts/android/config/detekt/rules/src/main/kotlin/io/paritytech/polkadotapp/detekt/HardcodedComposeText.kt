package io.paritytech.polkadotapp.detekt

import dev.detekt.api.Config
import dev.detekt.api.Entity
import dev.detekt.api.Finding
import dev.detekt.api.Rule
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtLiteralStringTemplateEntry
import org.jetbrains.kotlin.psi.KtNamedFunction
import org.jetbrains.kotlin.psi.KtStringTemplateExpression
import org.jetbrains.kotlin.psi.KtValueArgument
import org.jetbrains.kotlin.psi.psiUtil.getStrictParentOfType

class HardcodedComposeText(config: Config) : Rule(
    config,
    "Reports string literals passed as named UI text arguments inside composables. Move the text to strings.xml and read it with stringResource."
) {

    override fun visitCallExpression(expression: KtCallExpression) {
        super.visitCallExpression(expression)

        val calleeName = expression.calleeExpression?.text ?: return
        if (!expression.isInsideComposable()) return

        expression.valueArguments.forEach { argument ->
            val parameter = argument.textParameter(calleeName) ?: return@forEach
            val literal = argument.getArgumentExpression() as? KtStringTemplateExpression ?: return@forEach

            if (literal.hasTranslatableText()) {
                report(
                    Finding(
                        Entity.from(literal),
                        "Hardcoded UI text passed to '$parameter'. Move it to a string resource and read it with stringResource."
                    )
                )
            }
        }
    }

    private fun KtCallExpression.isInsideComposable(): Boolean {
        val function = getStrictParentOfType<KtNamedFunction>() ?: return false

        return function.hasAnnotation("Composable") && !function.hasAnnotation("Preview")
    }

    private fun KtNamedFunction.hasAnnotation(shortName: String) =
        annotationEntries.any { it.shortName?.asString() == shortName }

    private fun KtValueArgument.textParameter(calleeName: String): String? {
        val name = getArgumentName()?.asName?.asString()

        return when {
            name == LABEL && calleeName in ANIMATION_CALLS -> null
            name in TEXT_PARAMETERS -> name
            else -> null
        }
    }

    private fun KtStringTemplateExpression.hasTranslatableText() =
        entries.any { it is KtLiteralStringTemplateEntry && it.text.any(Char::isLetter) }

    private companion object {
        const val LABEL = "label"

        val TEXT_PARAMETERS = setOf("text", "title", "subtitle", LABEL, "placeholder", "headerText")

        val ANIMATION_CALLS = setOf(
            "Animatable",
            "AnimatedContent",
            "AnimatedVisibility",
            "Crossfade",
            "animateColor",
            "animateColorAsState",
            "animateDp",
            "animateDpAsState",
            "animateEnterExit",
            "animateFloat",
            "animateFloatAsState",
            "animateInt",
            "animateIntAsState",
            "animateIntOffset",
            "animateIntOffsetAsState",
            "animateIntSize",
            "animateIntSizeAsState",
            "animateOffset",
            "animateOffsetAsState",
            "animateRect",
            "animateRectAsState",
            "animateSize",
            "animateSizeAsState",
            "animateValue",
            "animateValueAsState",
            "createChildTransition",
            "createDeferredAnimation",
            "rememberInfiniteTransition",
            "rememberTransition",
            "updateTransition",
        )
    }
}
