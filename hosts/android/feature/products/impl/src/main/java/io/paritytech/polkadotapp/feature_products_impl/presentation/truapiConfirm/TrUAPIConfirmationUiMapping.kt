package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm

import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIConfirmation
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Renders each core review as a title plus typed detail rows. Exhaustive, so a
 * new confirmation variant cannot reach the user as an unlabelled prompt.
 * Signing is not here: it goes to the app's own signing sheet.
 */
fun TrUAPIConfirmation.Prompt.toUiState(): TrUAPIConfirmationUiState = when (this) {
    is TrUAPIConfirmation.StatementSign -> build(
        RCommon.string.truapi_confirm_title_statement_sign,
        detail(RCommon.string.truapi_confirm_label_payload_size, "$payloadSize"),
    )

    is TrUAPIConfirmation.AccountAlias -> build(
        RCommon.string.truapi_confirm_title_account_alias,
        detail(RCommon.string.truapi_confirm_label_context, proofContext),
        detail(RCommon.string.truapi_confirm_label_ring, ring),
    )

    is TrUAPIConfirmation.CreateProof -> build(
        RCommon.string.truapi_confirm_title_create_proof,
        detail(RCommon.string.truapi_confirm_label_context, proofContext),
        detail(RCommon.string.truapi_confirm_label_ring, ring),
        detail(RCommon.string.truapi_confirm_label_message_size, "$messageSize"),
    )

    is TrUAPIConfirmation.IdentityDisclosure -> build(
        RCommon.string.truapi_confirm_title_identity_disclosure,
    )

    is TrUAPIConfirmation.ResourceAllocation -> build(
        RCommon.string.truapi_confirm_title_resource_allocation,
        *resources.map { detail(RCommon.string.truapi_confirm_label_resource, it) }.toTypedArray(),
    )

    is TrUAPIConfirmation.PreimageSubmit -> build(
        RCommon.string.truapi_confirm_title_preimage_submit,
        detail(RCommon.string.truapi_confirm_label_payload_size, "$sizeBytes"),
    )

    is TrUAPIConfirmation.AccountAccess -> build(
        RCommon.string.truapi_confirm_title_account_access,
        detail(RCommon.string.truapi_confirm_label_target_product, targetProductId),
    )

    is TrUAPIConfirmation.ProductSubtree -> build(
        RCommon.string.truapi_confirm_title_product_subtree,
    )
}

private fun TrUAPIConfirmation.Prompt.build(
    titleRes: Int,
    vararg details: TrUAPIConfirmationDetail,
) = TrUAPIConfirmationUiState(
    titleRes = titleRes,
    productId = requesterProductId,
    details = details.toList().toImmutableList(),
)

private fun detail(labelRes: Int, value: String) =
    TrUAPIConfirmationDetail(labelRes, DetailValue.Text(value))

private fun detailRes(labelRes: Int, valueRes: Int) =
    TrUAPIConfirmationDetail(labelRes, DetailValue.Resource(valueRes))
