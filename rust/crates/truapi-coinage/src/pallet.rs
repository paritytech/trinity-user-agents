// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

use parity_scale_codec::{Compact, Encode};
use serde_json::{Value, json};

/// The pallet name every call and storage path hangs off.
pub const PALLET_NAME: &str = "Coinage";

/// The transaction-extension identifier of the coinage origins.
pub const AS_COINAGE_EXTENSION_ID: &str = "AsCoinage";

pub mod storage {
    pub const COINS_BY_OWNER: &str = "CoinsByOwner";
    pub const RECYCLERS_COIN_TO_RECYCLER: &str = "RecyclersCoinToRecycler";
    /// Alias state storage. The Host resolves legacy denomination/ring/alias
    /// keys or asset-instance-prefixed keys from runtime metadata.
    pub const RECYCLER_ALIAS_STATES: &str = "RecyclerAliasStates";
    pub const CONSUMED_FREE_UNLOAD_TOKENS: &str = "ConsumedFreeUnloadTokens";
}

/// Call names, exactly as the runtime metadata spells them.
pub mod calls {
    pub const LOAD_EXTERNAL_ASSET_UNPAID_BATCH: &str =
        "load_recycler_with_external_asset_unpaid_batch";
    pub const SPLIT: &str = "split";
    pub const UNLOAD_RECYCLER_INTO_COINS: &str = "unload_recycler_into_coins";
    pub const LOAD_RECYCLER_WITH_COIN: &str = "load_recycler_with_coin";
    pub const UNLOAD_RECYCLER_INTO_EXTERNAL_ASSET: &str = "unload_recycler_into_external_asset";
    /// 2026-08 upgrade rename of `…_and_vouchers`.
    pub const UNLOAD_RECYCLER_INTO_EXTERNAL_ASSET_AND_LOADED_COINS: &str =
        "unload_recycler_into_external_asset_and_loaded_coins";
    pub const TRANSFER: &str = "transfer";
}

/// `Coinage.load_recycler_with_coin(member_key, proof_of_ownership)`.
/// Both arguments are fixed byte arrays in the live metadata, so their
/// SCALE representation is the raw bytes with no compact length prefix.
/// Pallet/call indices are resolved from runtime metadata by the host.
pub fn load_recycler_with_coin_call(
    pallet_index: u8,
    call_index: u8,
    member_key: &[u8; 32],
    proof_of_ownership: &[u8; 64],
) -> Vec<u8> {
    let mut call = Vec::with_capacity(2 + member_key.len() + proof_of_ownership.len());
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(member_key);
    call.extend_from_slice(proof_of_ownership);
    call
}

/// `Coinage.transfer(to)`.
/// The live runtime declares `to` as one fixed 32-byte account id. The
/// source coin is not an argument: it is selected by the signed
/// `AsCoinage(Some(AsCoin))` transaction extension and the extrinsic
/// signer's sr25519 account id. Keeping this builder fixed-width prevents
/// accidentally encoding the recipient as a SCALE `Vec<u8>`.
pub fn transfer_call(pallet_index: u8, call_index: u8, recipient: &[u8; 32]) -> Vec<u8> {
    let mut call = Vec::with_capacity(2 + recipient.len());
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(recipient);
    call
}

/// Physical Members collection for a runtime-selected Coinage ABI.
/// `None` uses the legacy denomination at byte 16; `Some` inserts the
/// little-endian asset instance at bytes 16..20 and moves denomination to
/// byte 20, matching native `RecyclerCollectionIdentifier`.
/// The Host validates the runtime denomination before calling this helper.
pub fn recycler_collection_identifier(instance_id: Option<u32>, coin_value: i16) -> [u8; 32] {
    let mut id = [0u8; 32];
    id[..16].copy_from_slice(b"coinage/recycler");
    let denomination_offset = if let Some(instance_id) = instance_id {
        id[16..20].copy_from_slice(&instance_id.to_le_bytes());
        20
    } else {
        16
    };
    id[denomination_offset] = coin_value.clamp(0, u8::MAX as i16) as u8;
    id
}

fn hex_value(bytes: &[u8]) -> Value {
    Value::String(format!("0x{}", hex::encode(bytes)))
}

fn string_number(value: impl ToString) -> Value {
    Value::String(value.to_string())
}

/// `Preservation` of an unpaid external-asset load (tagged union, payload
/// always null).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preservation {
    Protect,
    Preserve,
    Expendable,
}

impl Preservation {
    fn to_json(self) -> Value {
        let tag = match self {
            Preservation::Protect => "Protect",
            Preservation::Preserve => "Preserve",
            Preservation::Expendable => "Expendable",
        };
        json!([tag, null])
    }

    /// Pinned from the live `CodecPreservation` metadata variant indices,
    /// which are NOT the declaration order of this enum.
    fn scale_index(self) -> u8 {
        match self {
            Self::Expendable => 0,
            Self::Protect => 1,
            Self::Preserve => 2,
        }
    }
}

/// One item of `load_recycler_with_external_asset_unpaid_batch`.
/// Field order and widths are pinned from the live
/// `indiv_pallet_coinage::pallet::UnpaidLoadInput` composite:
/// `preservation: CodecPreservation, value: i8, member_key: [u8; 32],
/// proof_of_ownership: [u8; 64]`. `value` is a coin exponent, so the
/// runtime's one-byte width is the whole domain (`MinimumExponent` 0..=
/// `MaximumExponent` 14).
#[derive(Debug, Clone)]
pub struct UnpaidLoadInput {
    pub value: i8,
    pub preservation: Preservation,
    pub member_key: [u8; 32],
    pub proof_of_ownership: Vec<u8>,
}

impl UnpaidLoadInput {
    pub fn to_json(&self) -> Value {
        json!({
            "value": string_number(self.value),
            "preservation": self.preservation.to_json(),
            "memberKey": hex_value(&self.member_key),
            "proofOfOwnership": hex_value(&self.proof_of_ownership),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SplitDestination {
    pub exponent: i16,
    pub accounts: Vec<[u8; 32]>,
}

impl SplitDestination {
    pub fn to_json(&self) -> Value {
        json!([
            string_number(self.exponent),
            self.accounts
                .iter()
                .map(|a| hex_value(a))
                .collect::<Vec<_>>(),
        ])
    }
}

/// The `split` call args (`split_into`).
pub fn split_args(split_into: &[SplitDestination]) -> Value {
    json!({
        "split_into": split_into.iter().map(SplitDestination::to_json).collect::<Vec<_>>(),
    })
}

/// SCALE call bytes for `Coinage.split(split_into)`.
/// Live metadata declares `split_into` as
/// `Vec<(i8, Vec<AccountId32>)>`. The public model retains `i16` so it can
/// share the denomination domain; this boundary rejects an exponent that the
/// runtime cannot represent rather than truncating it.
pub fn split_call(
    pallet_index: u8,
    call_index: u8,
    split_into: &[SplitDestination],
) -> Result<Vec<u8>, String> {
    let count = u32::try_from(split_into.len())
        .map_err(|_| "Coinage split destination list is too large".to_string())?;
    let mut call = Vec::new();
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(&Compact(count).encode());
    for destination in split_into {
        let exponent = i8::try_from(destination.exponent).map_err(|_| {
            format!(
                "Coinage split exponent {} does not fit the live i8 field",
                destination.exponent
            )
        })?;
        call.push(exponent as u8);
        call.extend_from_slice(&destination.accounts.encode());
    }
    Ok(call)
}

/// The batch-load call args (`items`).
pub fn load_external_asset_unpaid_batch_args(items: &[UnpaidLoadInput]) -> Value {
    json!({
        "items": items.iter().map(UnpaidLoadInput::to_json).collect::<Vec<_>>(),
    })
}

/// SCALE call bytes for
/// `Coinage.load_recycler_with_external_asset_unpaid_batch(items)`.
/// Runtime metadata pins each item as
/// `(Preservation, i8, [u8; 32], [u8; 64])`. Keeping the proof
/// length check at this boundary prevents a JSON-era `Vec<u8>` assumption
/// from silently producing a different call layout.
pub fn load_external_asset_unpaid_batch_call(
    pallet_index: u8,
    call_index: u8,
    items: &[UnpaidLoadInput],
) -> Result<Vec<u8>, String> {
    let item_count = u32::try_from(items.len())
        .map_err(|_| "Coinage unpaid voucher batch is too large".to_string())?;
    let mut call = Vec::with_capacity(2 + 5 + items.len().saturating_mul(99));
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(&Compact(item_count).encode());
    for item in items {
        let proof: &[u8; 64] = item.proof_of_ownership.as_slice().try_into().map_err(
            |_: std::array::TryFromSliceError| {
                format!(
                    "Coinage unpaid voucher proof has {} bytes; expected 64",
                    item.proof_of_ownership.len()
                )
            },
        )?;
        // `preservation` precedes `value` in the runtime composite, and
        // `value` is one byte — the reverse of both was a silent
        // field-shift that made the node's decode panic.
        call.push(item.preservation.scale_index());
        call.extend_from_slice(&item.value.to_le_bytes());
        call.extend_from_slice(&item.member_key);
        call.extend_from_slice(proof);
    }
    Ok(call)
}

fn aliases_json(aliases: &[Vec<u8>]) -> Vec<Value> {
    aliases.iter().map(|alias| hex_value(alias)).collect()
}

#[derive(Debug, Clone)]
pub struct UnloadRecyclerIntoCoinsArgs {
    pub aliases: Vec<Vec<u8>>,
    pub value: i8,
    pub index: u32,
    pub revision: u32,
    pub split_into: Vec<SplitDestination>,
    pub max_fee: u128,
}

impl UnloadRecyclerIntoCoinsArgs {
    pub fn to_json(&self) -> Value {
        json!({
            "aliases": aliases_json(&self.aliases),
            "value": string_number(self.value),
            "index": string_number(self.index),
            "revision": string_number(self.revision),
            "split_into": self
                .split_into
                .iter()
                .map(SplitDestination::to_json)
                .collect::<Vec<_>>(),
            "max_fee": string_number(self.max_fee),
        })
    }
}

/// SCALE call bytes for `Coinage.unload_recycler_into_coins`.
/// Legacy field order and widths are:
/// `Vec<[u8;32]>, i8, u32, u32, Vec<(i8, Vec<AccountId32>)>, u128`.
/// For the asset-instance ABI, `Some(instance_id)` inserts a fixed-width
/// little-endian `u32` before aliases, with no SCALE `Option` discriminant.
/// The Host must select the ABI and instance from metadata and trusted config.
/// Aliases are accepted as fixed arrays here so a JSON-era variable-length
/// value cannot shift every following field.
#[allow(clippy::too_many_arguments)]
pub fn unload_recycler_into_coins_call(
    pallet_index: u8,
    call_index: u8,
    instance_id: Option<u32>,
    aliases: &[[u8; 32]],
    value: i8,
    index: u32,
    revision: u32,
    split_into: &[SplitDestination],
    max_fee: u128,
) -> Result<Vec<u8>, String> {
    let mut call = Vec::new();
    call.extend_from_slice(&[pallet_index, call_index]);
    if let Some(instance_id) = instance_id {
        call.extend_from_slice(&instance_id.to_le_bytes());
    }
    call.extend_from_slice(&aliases.encode());
    call.push(value as u8);
    call.extend_from_slice(&index.to_le_bytes());
    call.extend_from_slice(&revision.to_le_bytes());

    let count = u32::try_from(split_into.len())
        .map_err(|_| "Coinage unload destination list is too large".to_string())?;
    call.extend_from_slice(&Compact(count).encode());
    for destination in split_into {
        let exponent = i8::try_from(destination.exponent).map_err(|_| {
            format!(
                "Coinage unload destination exponent {} does not fit the live i8 field",
                destination.exponent
            )
        })?;
        call.push(exponent as u8);
        call.extend_from_slice(&destination.accounts.encode());
    }
    call.extend_from_slice(&max_fee.to_le_bytes());
    Ok(call)
}

/// Arguments for `Coinage.unload_recycler_into_external_asset`.
#[derive(Debug, Clone)]
pub struct UnloadRecyclerIntoExternalAssetArgs {
    pub aliases: Vec<Vec<u8>>,
    pub value: i8,
    pub index: u32,
    pub revision: u32,
    pub to: [u8; 32],
}

impl UnloadRecyclerIntoExternalAssetArgs {
    pub fn to_json(&self) -> Value {
        json!({
            "aliases": aliases_json(&self.aliases),
            "value": string_number(self.value),
            "index": string_number(self.index),
            "revision": string_number(self.revision),
            "to": hex_value(&self.to),
        })
    }
}

/// SCALE call bytes for `unload_recycler_into_external_asset`.
/// Live metadata: `Vec<[u8; 32]>`, `i8`, `u32`, `u32`, `[u8; 32]`.
pub fn unload_recycler_into_external_asset_call(
    pallet_index: u8,
    call_index: u8,
    aliases: &[[u8; 32]],
    value: i8,
    index: u32,
    revision: u32,
    to: &[u8; 32],
) -> Vec<u8> {
    let mut call = Vec::new();
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(&aliases.encode());
    call.push(value as u8);
    call.extend_from_slice(&index.to_le_bytes());
    call.extend_from_slice(&revision.to_le_bytes());
    call.extend_from_slice(to);
    call
}

#[derive(Debug, Clone)]
pub struct NewVoucher {
    pub coin_value: i8,
    pub member_key: [u8; 32],
}

impl NewVoucher {
    pub fn to_json(&self) -> Value {
        json!([string_number(self.coin_value), hex_value(&self.member_key),])
    }
}

/// Arguments for
/// `Coinage.unload_recycler_into_external_asset_and_vouchers`.
#[derive(Debug, Clone)]
pub struct UnloadRecyclerIntoExternalAssetAndVouchersArgs {
    pub aliases: Vec<Vec<u8>>,
    pub value: i8,
    pub index: u32,
    pub revision: u32,
    pub to: [u8; 32],
    pub external_asset_amount: u128,
    pub new_vouchers: Vec<NewVoucher>,
}

impl UnloadRecyclerIntoExternalAssetAndVouchersArgs {
    pub fn to_json(&self) -> Value {
        json!({
            "aliases": aliases_json(&self.aliases),
            "value": string_number(self.value),
            "index": string_number(self.index),
            "revision": string_number(self.revision),
            "to": hex_value(&self.to),
            "external_asset_amount": string_number(self.external_asset_amount),
            "new_vouchers": self
                .new_vouchers
                .iter()
                .map(NewVoucher::to_json)
                .collect::<Vec<_>>(),
        })
    }
}

/// SCALE call bytes for
/// `unload_recycler_into_external_asset_and_vouchers`.
/// The surplus entries are unkeyed `(i8, [u8; 32])` tuples and the external
/// amount is a fixed-width little-endian `u128`, as pinned by live metadata.
#[allow(clippy::too_many_arguments)]
pub fn unload_recycler_into_external_asset_and_vouchers_call(
    pallet_index: u8,
    call_index: u8,
    aliases: &[[u8; 32]],
    value: i8,
    index: u32,
    revision: u32,
    to: &[u8; 32],
    external_asset_amount: u128,
    new_vouchers: &[NewVoucher],
) -> Vec<u8> {
    let mut call = Vec::new();
    call.extend_from_slice(&[pallet_index, call_index]);
    call.extend_from_slice(&aliases.encode());
    call.push(value as u8);
    call.extend_from_slice(&index.to_le_bytes());
    call.extend_from_slice(&revision.to_le_bytes());
    call.extend_from_slice(to);
    call.extend_from_slice(&external_asset_amount.to_le_bytes());
    call.extend_from_slice(&Compact(new_vouchers.len() as u32).encode());
    for voucher in new_vouchers {
        call.push(voucher.coin_value as u8);
        call.extend_from_slice(&voucher.member_key);
    }
    call
}

/// The personhood half of an unload-token proof: the ring-VRF proof and
/// the ring it opens against.
#[derive(Debug, Clone)]
pub struct PeopleProof {
    pub proof: Vec<u8>,
    pub ring: u32,
    /// The ring revision the proof was built against — required by the
    /// 2026-08 runtime's `MembershipProof` (spec 1000032).
    pub revision: u32,
}

impl PeopleProof {
    fn to_json(&self) -> Value {
        json!({
            "proof": hex_value(&self.proof),
            "ring": string_number(self.ring),
            "revision": string_number(self.revision),
        })
    }
}

#[derive(Debug, Clone)]
pub enum AsCoinageMode {
    /// Sr25519 coin-keypair origin — the variant tag is the whole payload.
    AsCoin,
    /// Full-person unload token: people-ring proof + per-voucher recycler
    /// alias proofs.
    AsUnloadTokenPeople {
        proof: PeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// Lite-person unload token (same payload as the full-person mode).
    AsUnloadTokenLitePeople {
        proof: PeopleProof,
        period: u32,
        counter: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// Pre-computed proof from the PAID unload-token ring.
    AsUnloadTokenPaid {
        proof: Vec<u8>,
        period: u32,
        paid_token_ring_index: u32,
        paid_token_ring_revision: u32,
        alias_proofs: Vec<Vec<u8>>,
    },
    /// Fee recycler output as the token (first alias proof = fee coin).
    /// `retry_counter` joined in the 2026-08 runtime (spec 1000032).
    AsUnloadTokenFromOutput {
        fee_recycler_value: i8,
        fee_recycler_index: u32,
        fee_recycler_revision: u32,
        retry_counter: u8,
        alias_proofs: Vec<Vec<u8>>,
    },
    InfallibleUnpaidSigned {
        nonce: u32,
    },
}

impl AsCoinageMode {
    /// The `["TagName", null | payload]` JSON the dynamic SCALE layer
    /// consumes.
    pub fn to_json(&self) -> Value {
        let alias_list =
            |proofs: &[Vec<u8>]| proofs.iter().map(|p| hex_value(p)).collect::<Vec<_>>();
        match self {
            AsCoinageMode::AsCoin => json!(["AsCoin", null]),
            AsCoinageMode::AsUnloadTokenPeople {
                proof,
                period,
                counter,
                alias_proofs,
            } => json!([
                "AsUnloadTokenPeople",
                {
                    "proof": proof.to_json(),
                    "period": string_number(period),
                    "counter": string_number(counter),
                    "aliasProofs": alias_list(alias_proofs),
                }
            ]),
            AsCoinageMode::AsUnloadTokenLitePeople {
                proof,
                period,
                counter,
                alias_proofs,
            } => json!([
                "AsUnloadTokenLitePeople",
                {
                    "proof": proof.to_json(),
                    "period": string_number(period),
                    "counter": string_number(counter),
                    "aliasProofs": alias_list(alias_proofs),
                }
            ]),
            AsCoinageMode::AsUnloadTokenPaid {
                proof,
                period,
                paid_token_ring_index,
                paid_token_ring_revision,
                alias_proofs,
            } => json!([
                "AsUnloadTokenPaid",
                {
                    "proof": hex_value(proof),
                    "period": string_number(period),
                    "paidTokenRingIndex": string_number(paid_token_ring_index),
                    "paidTokenRingRevision": string_number(paid_token_ring_revision),
                    "aliasProofs": alias_list(alias_proofs),
                }
            ]),
            AsCoinageMode::AsUnloadTokenFromOutput {
                fee_recycler_value,
                fee_recycler_index,
                fee_recycler_revision,
                retry_counter,
                alias_proofs,
            } => json!([
                "AsUnloadTokenFromOutput",
                {
                    "feeRecyclerValue": string_number(fee_recycler_value),
                    "feeRecyclerIndex": string_number(fee_recycler_index),
                    "feeRecyclerRevision": string_number(fee_recycler_revision),
                    "retryCounter": string_number(retry_counter),
                    "aliasProofs": alias_list(alias_proofs),
                }
            ]),
            AsCoinageMode::InfallibleUnpaidSigned { nonce } => json!([
                "InfallibleUnpaidSigned",
                { "nonce": string_number(nonce) }
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_recycler_with_coin_call_has_fixed_array_arguments() {
        let call = load_recycler_with_coin_call(52, 2, &[0x11; 32], &[0x22; 64]);
        assert_eq!(call.len(), 98);
        assert_eq!(&call[..2], &[52, 2]);
        assert_eq!(&call[2..34], &[0x11; 32]);
        assert_eq!(&call[34..], &[0x22; 64]);
    }

    #[test]
    fn unpaid_external_asset_batch_call_matches_fixed_runtime_layout() {
        let call = load_external_asset_unpaid_batch_call(
            52,
            1,
            &[
                UnpaidLoadInput {
                    value: -2,
                    preservation: Preservation::Expendable,
                    member_key: [0x11; 32],
                    proof_of_ownership: vec![0x22; 64],
                },
                UnpaidLoadInput {
                    value: 3,
                    preservation: Preservation::Protect,
                    member_key: [0x33; 32],
                    proof_of_ownership: vec![0x44; 64],
                },
            ],
        )
        .unwrap();
        // Pinned from the live `UnpaidLoadInput` composite: preservation
        // (1 byte, live variant indices) then value (1 byte), member key,
        // proof. Getting either the order or the value width wrong shifts
        // every later field and the node's decode panics.
        assert_eq!(&call[..3], &[52, 1, 8], "two-item Compact<u32>");
        assert_eq!(call[3], 0, "Expendable variant");
        assert_eq!(call[4] as i8, -2i8);
        assert_eq!(&call[5..37], &[0x11; 32]);
        assert_eq!(&call[37..101], &[0x22; 64]);
        assert_eq!(call[101], 1, "Protect variant");
        assert_eq!(call[102] as i8, 3i8);
        assert_eq!(&call[103..135], &[0x33; 32]);
        assert_eq!(&call[135..199], &[0x44; 64]);
        assert_eq!(call.len(), 199, "2 + 1 + 2 * (1 + 1 + 32 + 64)");
    }

    #[test]
    fn unpaid_external_asset_batch_rejects_variable_length_proof() {
        let error = load_external_asset_unpaid_batch_call(
            1,
            2,
            &[UnpaidLoadInput {
                value: 0,
                preservation: Preservation::Preserve,
                member_key: [0; 32],
                proof_of_ownership: vec![0; 63],
            }],
        )
        .unwrap_err();
        assert!(error.contains("63 bytes; expected 64"));
    }

    #[test]
    fn transfer_call_has_one_fixed_account_argument() {
        let call = transfer_call(52, 6, &[0xA5; 32]);
        assert_eq!(call.len(), 34);
        assert_eq!(&call[..2], &[52, 6]);
        assert_eq!(&call[2..], &[0xA5; 32]);
    }

    #[test]
    fn split_call_matches_live_scale_layout() {
        let call = split_call(
            52,
            3,
            &[
                SplitDestination {
                    exponent: -1,
                    accounts: vec![[0x11; 32], [0x22; 32]],
                },
                SplitDestination {
                    exponent: 4,
                    accounts: vec![[0x33; 32]],
                },
            ],
        )
        .unwrap();
        assert_eq!(&call[..3], &[52, 3, 8], "two destination tuples");
        assert_eq!(call[3], 0xff, "i8 exponent");
        assert_eq!(call[4], 8, "two accounts");
        assert_eq!(&call[5..37], &[0x11; 32]);
        assert_eq!(&call[37..69], &[0x22; 32]);
        assert_eq!(call[69], 4);
        assert_eq!(call[70], 4, "one account");
        assert_eq!(&call[71..103], &[0x33; 32]);
        assert_eq!(call.len(), 103);
    }

    #[test]
    fn unload_into_coins_call_matches_legacy_scale_layout() {
        let call = unload_recycler_into_coins_call(
            52,
            4,
            None,
            &[[0xAA; 32], [0xBB; 32]],
            -2,
            0x1122_3344,
            0x5566_7788,
            &[SplitDestination {
                exponent: 3,
                accounts: vec![[0xCC; 32]],
            }],
            9,
        )
        .unwrap();
        assert_eq!(&call[..3], &[52, 4, 8], "two aliases");
        assert_eq!(&call[3..35], &[0xAA; 32]);
        assert_eq!(&call[35..67], &[0xBB; 32]);
        assert_eq!(call[67], 0xfe);
        assert_eq!(&call[68..72], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&call[72..76], &0x5566_7788u32.to_le_bytes());
        assert_eq!(call[76], 4, "one split tuple");
        assert_eq!(call[77], 3);
        assert_eq!(call[78], 4, "one account");
        assert_eq!(&call[79..111], &[0xCC; 32]);
        assert_eq!(&call[111..], &9u128.to_le_bytes());
        assert_eq!(call.len(), 127);
    }

    #[test]
    fn unload_into_coins_call_matches_asset_instance_scale_layout() {
        let call = unload_recycler_into_coins_call(
            52,
            4,
            Some(0x1122_3344),
            &[[0xAA; 32], [0xBB; 32]],
            -2,
            0x5566_7788,
            0x99AA_BBCC,
            &[SplitDestination {
                exponent: 3,
                accounts: vec![[0xCC; 32]],
            }],
            9,
        )
        .unwrap();
        // The complete runtime argument tuple catches an Option tag, misplaced
        // instance, variable-width alias or shifted trailing argument.
        let expected = (
            52u8,
            4u8,
            0x1122_3344u32,
            vec![[0xAAu8; 32], [0xBB; 32]],
            -2i8,
            0x5566_7788u32,
            0x99AA_BBCCu32,
            vec![(3i8, vec![[0xCCu8; 32]])],
            9u128,
        )
            .encode();
        assert_eq!(call, expected);
    }

    #[test]
    fn coin_output_calls_reject_exponents_outside_live_i8() {
        for exponent in [-129, 128] {
            let destinations = [SplitDestination {
                exponent,
                accounts: vec![[0; 32]],
            }];
            assert!(split_call(1, 2, &destinations).is_err());
            for instance_id in [None, Some(0x1122_3344)] {
                assert!(
                    unload_recycler_into_coins_call(
                        1,
                        2,
                        instance_id,
                        &[],
                        0,
                        0,
                        0,
                        &destinations,
                        0,
                    )
                    .is_err()
                );
            }
        }
    }

    #[test]
    fn recycler_collection_matches_legacy_and_asset_instance_wire_layouts() {
        assert_eq!(
            recycler_collection_identifier(None, 5),
            *b"coinage/recycler\x05\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"
        );
        assert_eq!(
            recycler_collection_identifier(Some(0x1122_3344), 5),
            *b"coinage/recycler\x44\x33\x22\x11\x05\0\0\0\0\0\0\0\0\0\0\0"
        );
    }

    #[test]
    fn as_coinage_modes_match_the_reference_json() {
        assert_eq!(AsCoinageMode::AsCoin.to_json(), json!(["AsCoin", null]));
        assert_eq!(
            AsCoinageMode::InfallibleUnpaidSigned { nonce: 7 }.to_json(),
            json!(["InfallibleUnpaidSigned", { "nonce": "7" }])
        );
        let people = AsCoinageMode::AsUnloadTokenPeople {
            proof: PeopleProof {
                proof: vec![0xAA],
                ring: 2,
                revision: 6,
            },
            period: 9,
            counter: 1,
            alias_proofs: vec![vec![0xBB], vec![0xCC]],
        }
        .to_json();
        assert_eq!(
            people,
            json!([
                "AsUnloadTokenPeople",
                {
                    "proof": { "proof": "0xaa", "ring": "2", "revision": "6" },
                    "period": "9",
                    "counter": "1",
                    "aliasProofs": ["0xbb", "0xcc"],
                }
            ])
        );
        let from_output = AsCoinageMode::AsUnloadTokenFromOutput {
            fee_recycler_value: -1,
            fee_recycler_index: 3,
            fee_recycler_revision: 4,
            retry_counter: 2,
            alias_proofs: vec![],
        }
        .to_json();
        assert_eq!(
            from_output[1]["feeRecyclerValue"],
            Value::String("-1".into())
        );
    }

    #[test]
    fn call_args_match_the_reference_json() {
        let split = split_args(&[SplitDestination {
            exponent: 3,
            accounts: vec![[1u8; 32]],
        }]);
        assert_eq!(split["split_into"][0][0], Value::String("3".into()));
        assert!(
            split["split_into"][0][1][0]
                .as_str()
                .unwrap()
                .starts_with("0x0101")
        );

        let batch = load_external_asset_unpaid_batch_args(&[UnpaidLoadInput {
            value: 2,
            preservation: Preservation::Expendable,
            member_key: [9u8; 32],
            proof_of_ownership: vec![0xDD],
        }]);
        let item = &batch["items"][0];
        assert_eq!(item["value"], Value::String("2".into()));
        assert_eq!(item["preservation"], json!(["Expendable", null]));
        assert_eq!(item["proofOfOwnership"], Value::String("0xdd".into()));
    }

    #[test]
    fn unload_call_args_match_the_reference_json() {
        let into_coins = UnloadRecyclerIntoCoinsArgs {
            aliases: vec![vec![0xAA; 32]],
            value: -2,
            index: 7,
            revision: 9,
            split_into: vec![SplitDestination {
                exponent: 3,
                accounts: vec![[0x11; 32]],
            }],
            max_fee: 0,
        }
        .to_json();
        assert_eq!(
            into_coins,
            json!({
                "aliases": [format!("0x{}", "aa".repeat(32))],
                "value": "-2",
                "index": "7",
                "revision": "9",
                "split_into": [[
                    "3",
                    [format!("0x{}", "11".repeat(32))]
                ]],
                "max_fee": "0",
            })
        );

        let into_asset = UnloadRecyclerIntoExternalAssetArgs {
            aliases: vec![vec![0xBB; 32], vec![0xCC; 32]],
            value: 4,
            index: 10,
            revision: 11,
            to: [0x22; 32],
        }
        .to_json();
        assert_eq!(into_asset["aliases"].as_array().unwrap().len(), 2);
        assert_eq!(into_asset["value"], "4");
        assert_eq!(into_asset["index"], "10");
        assert_eq!(into_asset["revision"], "11");
        assert_eq!(
            into_asset["to"],
            Value::String(format!("0x{}", "22".repeat(32)))
        );

        let with_change = UnloadRecyclerIntoExternalAssetAndVouchersArgs {
            aliases: vec![vec![0xDD; 32]],
            value: 5,
            index: 12,
            revision: 13,
            to: [0x33; 32],
            external_asset_amount: u128::MAX,
            new_vouchers: vec![NewVoucher {
                coin_value: -1,
                member_key: [0x44; 32],
            }],
        }
        .to_json();
        assert_eq!(
            with_change["external_asset_amount"],
            Value::String(u128::MAX.to_string())
        );
        assert_eq!(
            with_change["new_vouchers"][0],
            json!(["-1", format!("0x{}", "44".repeat(32)),])
        );
        assert!(with_change.get("externalAssetAmount").is_none());
        assert!(with_change.get("newVouchers").is_none());
    }

    #[test]
    fn external_asset_unload_calls_match_live_scale_layout() {
        let aliases = [[0x11; 32], [0x22; 32]];
        let plain =
            unload_recycler_into_external_asset_call(68, 5, &aliases, -2, 7, 9, &[0x33; 32]);
        assert_eq!(&plain[..3], &[68, 5, 8], "two aliases, compact len 2");
        assert_eq!(&plain[3..35], &[0x11; 32]);
        assert_eq!(&plain[35..67], &[0x22; 32]);
        assert_eq!(plain[67], (-2i8) as u8);
        assert_eq!(&plain[68..72], &7u32.to_le_bytes());
        assert_eq!(&plain[72..76], &9u32.to_le_bytes());
        assert_eq!(&plain[76..], &[0x33; 32]);

        let with_change = unload_recycler_into_external_asset_and_vouchers_call(
            68,
            9,
            &aliases[..1],
            4,
            10,
            11,
            &[0x44; 32],
            123,
            &[NewVoucher {
                coin_value: -1,
                member_key: [0x55; 32],
            }],
        );
        assert_eq!(&with_change[..3], &[68, 9, 4], "one alias");
        let amount_offset = 3 + 32 + 1 + 4 + 4 + 32;
        assert_eq!(
            &with_change[amount_offset..amount_offset + 16],
            &123u128.to_le_bytes()
        );
        assert_eq!(with_change[amount_offset + 16], 4, "one voucher");
        assert_eq!(with_change[amount_offset + 17], (-1i8) as u8);
        assert_eq!(&with_change[amount_offset + 18..], &[0x55; 32]);
    }
}
