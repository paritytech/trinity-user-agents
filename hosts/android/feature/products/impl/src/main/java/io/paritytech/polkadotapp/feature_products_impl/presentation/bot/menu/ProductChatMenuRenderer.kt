package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import io.paritytech.polkadotapp.common.utils.capitalize
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDragHandler
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatPlaceholder
import io.paritytech.polkadotapp.design.components.icon.vectors.Leave
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMenuRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.common.R as RCommon

internal class ProductChatMenuRenderer(
    private val product: Product,
    private val chatId: ChatId,
) : CustomChatMenuRenderer {
    @Composable
    override fun DrawMenu(onDismiss: () -> Unit) {
        val viewModel = hiltViewModel<ProductChatMenuViewModel>()
        val currentPage by viewModel.currentPage.collectAsState()

        DisposableEffect(Unit) {
            onDispose { viewModel.onMenuDismissed() }
        }

        AnimatedContent(
            targetState = currentPage,
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC }
        ) { page ->
            when (page) {
                ProductChatMenuPage.MAIN -> MainContent(
                    appName = product.name,
                    onOpenAppClick = {
                        viewModel.onOpenAppClick(product.id)
                        onDismiss()
                    },
                    onRemoveChatClick = { viewModel.onRemoveChatClick() }
                )

                ProductChatMenuPage.REMOVE_CONFIRMATION -> RemoveConfirmationContent(
                    onConfirm = { viewModel.onRemoveChatConfirmed(product.id, chatId) },
                    onCancel = { onDismiss() },
                )
            }
        }
    }
}

@Composable
private fun MainContent(
    appName: String,
    onOpenAppClick: () -> Unit,
    onRemoveChatClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.mediumIncreased
            ),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaBottomSheetDragHandler()

        NovaMenuOption(
            text = stringResource(RCommon.string.product_chat_menu_open_app, appName.capitalize()),
            icon = NovaIcons.ChatPlaceholder,
            color = PolkadotTheme.colors.fg.primary,
            onClick = onOpenAppClick
        )

        NovaMenuOption(
            text = stringResource(RCommon.string.product_chat_menu_remove_chat),
            icon = NovaIcons.Leave,
            color = PolkadotTheme.colors.fg.error,
            onClick = onRemoveChatClick
        )
    }
}

@Composable
private fun RemoveConfirmationContent(
    onConfirm: () -> Unit,
    onCancel: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            modifier = Modifier.padding(top = PolkadotTheme.spacings.small),
            text = stringResource(RCommon.string.product_chat_menu_remove_confirmation_text),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { extraLarge }

        Row(
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            val buttonModifier = Modifier.weight(1f)

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.common_cancel),
                style = PolkadotButtonStyle.secondary(),
                onClick = onCancel,
            )

            PolkadotTextButton(
                modifier = buttonModifier,
                text = stringResource(RCommon.string.product_chat_menu_remove_confirmation_positive),
                style = PolkadotButtonStyle.destructive(),
                onClick = onConfirm,
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MainContentPreview() {
    PolkadotTheme {
        MainContent(
            appName = "Nova Swap",
            onOpenAppClick = {},
            onRemoveChatClick = {}
        )
    }
}
