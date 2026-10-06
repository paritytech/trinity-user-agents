//! Withdrawals moved from People to Asset Hub, the way getcash's withdrawal
//! tick moves them.
//!
//! A paid withdrawal account holds only CASH. People has no XCM exchanger,
//! so a pool swap paid for in CASH first buys the PAS the XCM's fees need.
//! The XCM comes second and pays in that PAS: it withdraws all the CASH and
//! the PAS, pays People's execution and delivery with an exact allowance,
//! and sends both to Asset Hub, where the program sells the CASH for PAS and
//! deposits it on the landing account. The XCM is sized by dry runs: one
//! pays People's fees with every PAS withdrawn and reads back what was left
//! as trapped, the next confirm an allowance that traps nothing, and Asset
//! Hub runs the forwarded program to say what lands.

use futures::future::BoxFuture;
use parity_scale_codec::Decode;
use subxt::dynamic::Value;
use subxt::tx::ValidationResult;

use super::{
    Chains, ConversionError, RuntimeCall, Sr25519Signer, XCM_VERSION, as_u128, beneficiary,
    call_api, chain, fetch_value, field, find_variant, fungible_total, items, junction, location,
    module_error_names, native, ok, sign_on, u128_at, unversioned, unwrap_newtype, variant_name,
    versioned, weight,
};

/// CASH set aside for Asset Hub's execution fee, above the one percent
/// floor, as getcash sets it; the unused part is refunded into the sale.
const ASSET_HUB_FEE_BUFFER_CASH: u128 = 10_000;
/// Headroom the swap may spend above the CASH the dry run took, percent.
const SWAP_HEADROOM_PERCENT: u128 = 2;
/// Headroom on the XCM's transaction fee estimate, percent.
const XCM_TX_FEE_HEADROOM_PERCENT: u128 = 5;
/// Rounds of measure and confirm before an exact People allowance is given
/// up on.
const FEE_ROUNDS: usize = 3;
/// An amount whose compact encoding is the longest any amount below 2^64
/// gets, so a stand-in call is no shorter than the real one.
const LONGEST_AMOUNT: u128 = 1 << 63;
/// How far below the quoted sale the Asset Hub price may move before the
/// program fails there, percent; a landing counts once it reaches the dry
/// run's less this.
pub const WITHDRAW_SLIPPAGE_PERCENT: u128 = 5;
/// Weight ceiling for the XCM when People will not weigh it.
const FALLBACK_MAX_WEIGHT: (u64, u64) = (5_000_000_000, 300_000);

/// How CASH moves from People to Asset Hub on the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CashTransfer {
    /// Both chains trust a teleport of CASH.
    Teleport,
    /// People withdraws CASH from its reserve on Asset Hub.
    Reserve,
}

/// A withdrawal transaction signed and ready to submit on People.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWithdrawal {
    /// The signed extrinsic.
    pub extrinsic: Vec<u8>,
    /// Last People block it can be included in.
    pub valid_until_block: u64,
    /// What it does.
    pub call: WithdrawCall,
}

/// Which of a withdrawal's two transactions a prepared one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawCall {
    /// The pool swap buying the PAS the XCM's fees need.
    Swap,
    /// The XCM moving everything to Asset Hub.
    Transfer {
        /// PAS the Asset Hub dry run credited to the landing account.
        expected_landing: u128,
    },
}

/// Why a transfer could not be sized.
enum Sizing {
    /// The account holds too little PAS for the XCM's fees: swap first.
    NeedsSwap,
    /// Anything else.
    Failed(ConversionError),
}

impl From<ConversionError> for Sizing {
    fn from(error: ConversionError) -> Self {
        Self::Failed(error)
    }
}

/// Who signs a withdrawal transaction, and where its funds go.
struct WithdrawSigning<'a> {
    extensions: &'a crate::runtime::statement_allowance::extension::Metadata,
    signer: &'a Sr25519Signer,
    nonce: u32,
    account: [u8; 32],
    landing: [u8; 32],
    transfer: CashTransfer,
}

/// The amounts and accounts one XCM is built from.
#[derive(Debug, Clone, Copy)]
struct Transfer {
    cash: u128,
    pas_to_withdraw: u128,
    pay_fees_pas: u128,
    remote_fees_cash: u128,
    min_pas_out: u128,
    landing: [u8; 32],
    claimer: [u8; 32],
    transfer: CashTransfer,
}

impl Chains {
    /// `account`'s CASH and PAS on People.
    pub async fn people_holdings(&self, account: &[u8; 32]) -> Result<(u128, u128), ConversionError> {
        let cash = self.people_cash(account).await?;
        let pas = match fetch_value(&self.people, "System", "Account", vec![Value::from_bytes(account)]).await? {
            Some(entry) => u128_at(field(&entry, "data")?, "free")?,
            None => 0,
        };
        Ok((cash, pas))
    }

    /// `account`'s next nonce on People.
    pub async fn people_nonce(&self, account: &[u8; 32]) -> Result<u32, ConversionError> {
        let nonce = call_api(
            &self.people,
            "AccountNonceApi",
            "account_nonce",
            vec![Value::from_bytes(account)],
        )
        .await?;
        u32::try_from(as_u128(&nonce)?).map_err(chain)
    }

    /// The finalized People block these reads are pinned to.
    pub fn people_block(&self) -> u64 {
        self.people.block_number()
    }

    /// `account`'s PAS on Asset Hub.
    pub async fn asset_hub_native(&self, account: &[u8; 32]) -> Result<u128, ConversionError> {
        self.asset_hub_balance(crate::host_logic::funding::DepositAsset::Native, account)
            .await
    }

    /// The next withdrawal transaction for the account `keypair` holds at
    /// `nonce`, landing on `landing`: the XCM when the account holds the PAS
    /// its fees need, the swap buying it otherwise.
    pub async fn prepare_withdrawal(
        &self,
        keypair: &schnorrkel::Keypair,
        nonce: u32,
        landing: [u8; 32],
    ) -> Result<PreparedWithdrawal, ConversionError> {
        let extensions = self
            .people_extensions
            .as_deref()
            .ok_or_else(|| chain("People's signing metadata was not loaded"))?;
        let signer = Sr25519Signer::from_keypair(keypair);
        let account = keypair.public.to_bytes();
        let (cash, pas) = self.people_holdings(&account).await?;
        if cash == 0 {
            return Err(ConversionError::Refused("the withdrawal account holds no CASH".into()));
        }
        let signing = WithdrawSigning {
            extensions,
            signer: &signer,
            nonce,
            account,
            landing,
            transfer: self.cash_transfer().await?,
        };
        if pas > 0 {
            match self.size_transfer(&signing, cash, pas).await {
                Ok(prepared) => return Ok(prepared),
                Err(Sizing::NeedsSwap) => {}
                Err(Sizing::Failed(error)) => return Err(error),
            }
        }
        self.size_swap(&signing, cash).await
    }

    /// Submit a signed People transaction, after People's transaction pool
    /// accepts it. Only the pool's refusal is a refusal.
    pub async fn submit_on_people(&self, extrinsic: Vec<u8>) -> Result<(), ConversionError> {
        let transaction = self.people.tx().from_bytes(extrinsic);
        match transaction.validate().await.map_err(chain)? {
            ValidationResult::Valid(_) => {}
            invalid => {
                return Err(ConversionError::Refused(format!(
                    "People rejects the transaction: {invalid:?}"
                )));
            }
        }
        transaction.submit().await.map(|_| ()).map_err(chain)
    }

    /// How CASH moves, as getcash chooses it from the trust each runtime
    /// enforces: teleport when Asset Hub trusts People as a CASH teleporter
    /// and People agrees or does not answer; reserve withdrawal when People
    /// trusts Asset Hub as CASH's reserve or does not answer; neither
    /// otherwise.
    async fn cash_transfer(&self) -> Result<CashTransfer, ConversionError> {
        let people_answers = self
            .people
            .metadata_ref()
            .runtime_api_trait_by_name("TrustedQueryApi")
            .is_some();
        let asset_hub_teleports = trusted(
            &self.asset_hub,
            "is_trusted_teleporter",
            self.places.asset(&self.places.cash(), 1),
            self.people_on_asset_hub(),
        )
        .await?;
        let people_trusts = |method| async move {
            match people_answers {
                false => Ok(true),
                true => {
                    trusted(
                        &self.people,
                        method,
                        self.places.asset(&self.places.cash_on_people(), 1),
                        self.asset_hub_from_people(),
                    )
                    .await
                }
            }
        };
        if asset_hub_teleports && people_trusts("is_trusted_teleporter").await? {
            return Ok(CashTransfer::Teleport);
        }
        if people_trusts("is_trusted_reserve").await? {
            return Ok(CashTransfer::Reserve);
        }
        Err(ConversionError::Refused(
            "this network does not let CASH move between People and Asset Hub".into(),
        ))
    }

    /// The swap buying People's existential deposit plus the XCM's fee in
    /// PAS, paid for in CASH, its cost capped at what a dry run spent plus
    /// headroom.
    async fn size_swap(&self, signing: &WithdrawSigning<'_>, cash: u128) -> Result<PreparedWithdrawal, ConversionError> {
        let account = signing.account;
        let stand_in = Transfer {
            cash,
            pas_to_withdraw: LONGEST_AMOUNT,
            pay_fees_pas: LONGEST_AMOUNT,
            remote_fees_cash: remote_fees_cash(cash),
            min_pas_out: LONGEST_AMOUNT,
            landing: signing.landing,
            claimer: account,
            transfer: signing.transfer,
        };
        let (_, reserve) = self.xcm_tx_fee_reserve(signing, &stand_in).await?;
        let pas_out = self.people_existential_deposit()?.saturating_add(reserve);
        let spent = self.dry_run_swap(&account, pas_out, cash).await?;
        let cash_in_max = spent.saturating_mul(100 + SWAP_HEADROOM_PERCENT).div_ceil(100);
        if cash_in_max >= cash {
            return Err(ConversionError::Refused(format!(
                "{cash} CASH cannot buy the {pas_out} PAS the withdrawal's fees need"
            )));
        }
        let call = swap_call(&self.places.cash_on_people(), &account, pas_out, cash_in_max);
        Ok(PreparedWithdrawal {
            extrinsic: sign_on(
                &self.people,
                signing.extensions,
                signing.signer,
                &call,
                &self.places.cash_on_people(),
                signing.nonce,
            )?,
            valid_until_block: self.people.block_number() + super::MORTAL_PERIOD_BLOCKS,
            call: WithdrawCall::Swap,
        })
    }

    /// The XCM moving all the account's CASH and PAS to Asset Hub, with
    /// People's fees paid by an exact allowance, as getcash sizes it.
    async fn size_transfer(
        &self,
        signing: &WithdrawSigning<'_>,
        cash: u128,
        pas: u128,
    ) -> Result<PreparedWithdrawal, Sizing> {
        let (account, landing, transfer) = (signing.account, signing.landing, signing.transfer);
        let quoted = self.quote_cash_for_pas(cash).await?;
        let base = |pas_to_withdraw, pay_fees_pas| Transfer {
            cash,
            pas_to_withdraw,
            pay_fees_pas,
            remote_fees_cash: remote_fees_cash(cash),
            min_pas_out: quoted / 100 * (100 - WITHDRAW_SLIPPAGE_PERCENT)
                + quoted % 100 * (100 - WITHDRAW_SLIPPAGE_PERCENT) / 100,
            landing,
            claimer: account,
            transfer,
        };
        let (max_weight, reserve) = self.xcm_tx_fee_reserve(signing, &base(pas, pas)).await?;
        if pas < self.people_existential_deposit()?.saturating_add(reserve) {
            return Err(Sizing::NeedsSwap);
        }
        let pas_to_withdraw = pas - reserve;
        let generous = base(pas_to_withdraw, pas_to_withdraw);
        let first = match self.dry_run_transfer(&account, &generous, &max_weight).await? {
            PeopleRun::Ran { trapped, forwarded } => (trapped, forwarded),
            PeopleRun::ShortOfFees => return Err(Sizing::NeedsSwap),
            PeopleRun::Refused(reason) => return Err(Sizing::Failed(ConversionError::Refused(reason))),
        };
        let charged = pas_to_withdraw.saturating_sub(first.0);
        let local = charged.saturating_sub(self.people_delivery_fee(&first.1).await?);
        let mut pay_fees_pas = local.saturating_add(
            self.people_delivery_fee(&forwarded_stand_in(&self.places, &base(pas_to_withdraw, charged)))
                .await?,
        );
        let mut confirmed = None;
        for _ in 0..FEE_ROUNDS {
            if pay_fees_pas >= pas_to_withdraw {
                return Err(Sizing::NeedsSwap);
            }
            let attempt = base(pas_to_withdraw, pay_fees_pas);
            match self.dry_run_transfer(&account, &attempt, &max_weight).await? {
                PeopleRun::Ran { trapped: 0, forwarded } => {
                    confirmed = Some((attempt, forwarded));
                    break;
                }
                PeopleRun::Ran { trapped, .. } => pay_fees_pas -= trapped.min(pay_fees_pas),
                PeopleRun::ShortOfFees => pay_fees_pas += pay_fees_pas / 100 + 1,
                PeopleRun::Refused(reason) => return Err(Sizing::Failed(ConversionError::Refused(reason))),
            }
        }
        let Some((final_transfer, forwarded)) = confirmed else {
            return Err(Sizing::Failed(ConversionError::Refused(
                "no exact People fee allowance for the withdrawal was found".into(),
            )));
        };
        let expected_landing = self.dry_run_landing(&forwarded, &landing).await?;
        let call = execute_call(&self.places, &final_transfer, max_weight);
        Ok(PreparedWithdrawal {
            extrinsic: sign_on(&self.people, signing.extensions, signing.signer, &call, &native(), signing.nonce)?,
            valid_until_block: self.people.block_number() + super::MORTAL_PERIOD_BLOCKS,
            call: WithdrawCall::Transfer { expected_landing },
        })
    }

    /// People's existential deposit in PAS.
    fn people_existential_deposit(&self) -> Result<u128, ConversionError> {
        let deposit = self
            .people
            .metadata_ref()
            .pallet_by_name("Balances")
            .and_then(|pallet| pallet.constant_by_name("ExistentialDeposit"))
            .ok_or_else(|| chain("People declares no existential deposit"))?
            .value();
        u128::decode(&mut &deposit[..]).map_err(chain)
    }

    /// The PAS Asset Hub's pools give for `cash` CASH now.
    async fn quote_cash_for_pas(&self, cash: u128) -> Result<u128, ConversionError> {
        let quoted = call_api(
            &self.asset_hub,
            "AssetConversionApi",
            "quote_price_exact_tokens_for_tokens",
            vec![self.places.cash(), native(), Value::u128(cash), Value::bool(true)],
        )
        .await?;
        let quoted = super::variant_fields(&quoted)
            .filter(|_| variant_name(&quoted) == Some("Some"))
            .and_then(|fields| fields.values().next())
            .ok_or_else(|| ConversionError::Refused("Asset Hub cannot quote the sale of the CASH".into()))?;
        as_u128(quoted)
    }

    /// The XCM's weight, as People weighs it, and its transaction fee in PAS
    /// with headroom, estimated on the call `transfer` builds.
    async fn xcm_tx_fee_reserve(
        &self,
        signing: &WithdrawSigning<'_>,
        transfer: &Transfer,
    ) -> Result<(Value, u128), ConversionError> {
        let message = withdraw_message(&self.places, transfer);
        let weighed = ok(call_api(&self.people, "XcmPaymentApi", "query_xcm_weight", vec![message]).await?)
            .unwrap_or_else(|_| weight(FALLBACK_MAX_WEIGHT.0, FALLBACK_MAX_WEIGHT.1));
        let call = execute_call(&self.places, transfer, weighed.clone());
        let extrinsic = sign_on(&self.people, signing.extensions, signing.signer, &call, &native(), signing.nonce)?;
        let fee = self.people_dispatch_fee(&extrinsic).await?;
        Ok((weighed, fee.saturating_mul(100 + XCM_TX_FEE_HEADROOM_PERCENT).div_ceil(100)))
    }

    /// What dispatching `extrinsic` on People costs, in PAS.
    async fn people_dispatch_fee(&self, extrinsic: &[u8]) -> Result<u128, ConversionError> {
        let unprefixed = Vec::<u8>::decode(&mut &extrinsic[..]).map_err(chain)?;
        let length = u32::try_from(extrinsic.len()).map_err(chain)?;
        let info = call_api(
            &self.people,
            "TransactionPaymentApi",
            "query_info",
            vec![Value::from_bytes(&unprefixed), Value::u128(length.into())],
        )
        .await?;
        u128_at(&info, "partial_fee")
    }

    /// What delivering `message` from People to Asset Hub costs, in PAS.
    async fn people_delivery_fee(&self, message: &Value) -> Result<u128, ConversionError> {
        let fees = ok(call_api(
            &self.people,
            "XcmPaymentApi",
            "query_delivery_fees",
            vec![versioned(self.asset_hub_from_people()), message.clone(), versioned(native())],
        )
        .await?)?;
        Ok(fungible_total(unversioned(&fees)?))
    }

    /// The CASH a swap for `pas_out` PAS takes from `account`, as a dry run
    /// on People spends it.
    async fn dry_run_swap(&self, account: &[u8; 32], pas_out: u128, cash: u128) -> Result<u128, ConversionError> {
        let call = swap_call(&self.places.cash_on_people(), account, pas_out, cash);
        let effects = self.dry_run_on_people_call(account, &call).await?;
        let execution = field(&effects, "execution_result")?;
        if variant_name(execution) != Some("Ok") {
            let names = module_error_names(self.people.metadata_ref(), execution);
            return Err(ConversionError::Refused(format!(
                "People would not swap CASH for the withdrawal's fees: {}",
                names.join(" / ")
            )));
        }
        let events = field(&effects, "emitted_events")?;
        items(events)
            .into_iter()
            .find_map(|event| find_variant(event, "SwapExecuted"))
            .map(|swapped| u128_at(swapped, "amount_in"))
            .ok_or_else(|| chain("the swap's dry run reported no swap"))?
    }

    /// Dry-run the XCM `transfer` builds on People.
    async fn dry_run_transfer(
        &self,
        account: &[u8; 32],
        transfer: &Transfer,
        max_weight: &Value,
    ) -> Result<PeopleRun, ConversionError> {
        let call = execute_call(&self.places, transfer, max_weight.clone());
        let effects = self.dry_run_on_people_call(account, &call).await?;
        let execution = field(&effects, "execution_result")?;
        if variant_name(execution) != Some("Ok") {
            let names = module_error_names(self.people.metadata_ref(), execution);
            if names.iter().any(|name| name == "NotHoldingFees" || name == "TooExpensive") {
                return Ok(PeopleRun::ShortOfFees);
            }
            return Ok(PeopleRun::Refused(format!(
                "People rejects the withdrawal: {} ({execution})",
                names.join(" / ")
            )));
        }
        let trapped = trapped_in(field(&effects, "emitted_events")?);
        let asset_hub = self.places.asset_hub_para;
        let forwarded = items(field(&effects, "forwarded_xcms")?)
            .into_iter()
            .find_map(|entry| {
                let [destination, messages] = items(entry)[..] else {
                    return None;
                };
                (super::parachain_of(destination) == Some(asset_hub))
                    .then(|| items(messages).first().map(|message| (*message).clone()))
                    .flatten()
            })
            .ok_or_else(|| ConversionError::Refused("the withdrawal forwards nothing to Asset Hub".into()))?;
        Ok(PeopleRun::Ran { trapped, forwarded })
    }

    /// `DryRunApi::dry_run_call` of `call` from `account` on People.
    async fn dry_run_on_people_call(&self, account: &[u8; 32], call: &RuntimeCall) -> Result<Value, ConversionError> {
        let origin = Value::unnamed_variant(
            "system",
            [Value::unnamed_variant("Signed", [Value::from_bytes(account)])],
        );
        ok(call_api(
            &self.people,
            "DryRunApi",
            "dry_run_call",
            vec![origin, call.value(), Value::u128(XCM_VERSION.into())],
        )
        .await?)
    }

    /// Run the forwarded program on Asset Hub as People: it must complete,
    /// trap nothing and credit `landing` PAS, which it returns.
    async fn dry_run_landing(&self, forwarded: &Value, landing: &[u8; 32]) -> Result<u128, ConversionError> {
        let effects = ok(call_api(
            &self.asset_hub,
            "DryRunApi",
            "dry_run_xcm",
            vec![versioned(self.people_on_asset_hub()), forwarded.clone()],
        )
        .await?)?;
        let outcome = field(&effects, "execution_result")?;
        if variant_name(outcome) != Some("Complete") {
            return Err(ConversionError::Refused(format!(
                "the withdrawal would not complete on Asset Hub: {outcome}"
            )));
        }
        let events = field(&effects, "emitted_events")?;
        if trapped_in(events) > 0 {
            return Err(ConversionError::Refused("the withdrawal would trap assets on Asset Hub".into()));
        }
        match credited_native(events, landing) {
            0 => Err(ConversionError::Refused("nothing of the withdrawal would reach Asset Hub".into())),
            landed => Ok(landed),
        }
    }

    /// People as Asset Hub sees it.
    fn people_on_asset_hub(&self) -> Value {
        location(1, vec![junction("Parachain", Value::u128(self.places.people_para.into()))])
    }

    /// Asset Hub as People sees it.
    fn asset_hub_from_people(&self) -> Value {
        location(1, vec![junction("Parachain", Value::u128(self.places.asset_hub_para.into()))])
    }
}

/// What moving a withdrawal to Asset Hub reads and does on the chains.
pub trait WithdrawChains: Send + Sync {
    /// `account`'s CASH and PAS on People.
    fn people_holdings<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<(u128, u128), ConversionError>>;
    /// `account`'s next nonce on People.
    fn people_nonce<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>>;
    /// The finalized People block these reads are pinned to.
    fn people_block(&self) -> u64;
    /// `account`'s PAS on Asset Hub.
    fn asset_hub_native<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>>;
    /// Size, dry-run and sign the next withdrawal transaction.
    fn prepare_withdrawal<'a>(
        &'a self,
        keypair: &'a schnorrkel::Keypair,
        nonce: u32,
        landing: [u8; 32],
    ) -> BoxFuture<'a, Result<PreparedWithdrawal, ConversionError>>;
}

impl WithdrawChains for Chains {
    fn people_holdings<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<(u128, u128), ConversionError>> {
        Box::pin(Chains::people_holdings(self, account))
    }

    fn people_nonce<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u32, ConversionError>> {
        Box::pin(Chains::people_nonce(self, account))
    }

    fn people_block(&self) -> u64 {
        Chains::people_block(self)
    }

    fn asset_hub_native<'a>(&'a self, account: &'a [u8; 32]) -> BoxFuture<'a, Result<u128, ConversionError>> {
        Box::pin(Chains::asset_hub_native(self, account))
    }

    fn prepare_withdrawal<'a>(
        &'a self,
        keypair: &'a schnorrkel::Keypair,
        nonce: u32,
        landing: [u8; 32],
    ) -> BoxFuture<'a, Result<PreparedWithdrawal, ConversionError>> {
        Box::pin(Chains::prepare_withdrawal(self, keypair, nonce, landing))
    }
}

/// A chain's `TrustedQueryApi` answer for `asset` from `location`.
async fn trusted(
    at: &subxt::client::OnlineClientAtBlock<subxt::config::substrate::SubstrateConfig>,
    method: &str,
    asset: Value,
    from: Value,
) -> Result<bool, ConversionError> {
    let answer = ok(call_api(at, "TrustedQueryApi", method, vec![versioned(asset), versioned(from)]).await?)?;
    match answer.value {
        subxt::ext::scale_value::ValueDef::Primitive(subxt::ext::scale_value::Primitive::Bool(trusted)) => Ok(trusted),
        _ => Err(chain(format!("TrustedQueryApi::{method} gave no answer: {answer}"))),
    }
}

/// What one dry run of the XCM on People found.
enum PeopleRun {
    /// It ran: what it left trapped, and the message it forwards to Asset
    /// Hub.
    Ran { trapped: u128, forwarded: Value },
    /// It could not pay its fees.
    ShortOfFees,
    /// It failed for another reason.
    Refused(String),
}

/// CASH the remote fee carries on Asset Hub: one percent, at least the
/// buffer, as getcash earmarks it.
fn remote_fees_cash(cash: u128) -> u128 {
    (cash / 100).max(ASSET_HUB_FEE_BUFFER_CASH)
}

/// The pool swap of up to `cash_in_max` CASH for exactly `pas_out` PAS,
/// credited to `account`.
fn swap_call(cash_on_people: &Value, account: &[u8; 32], pas_out: u128, cash_in_max: u128) -> RuntimeCall {
    RuntimeCall::new(
        "AssetConversion",
        "swap_tokens_for_exact_tokens",
        vec![
            ("path", Value::unnamed_composite([cash_on_people.clone(), native()])),
            ("amount_out", Value::u128(pas_out)),
            ("amount_in_max", Value::u128(cash_in_max)),
            ("send_to", Value::from_bytes(account)),
            ("keep_alive", Value::bool(false)),
        ],
    )
}

/// The XCM transaction for `transfer`, weighed at `max_weight`.
fn execute_call(places: &super::Places, transfer: &Transfer, max_weight: Value) -> RuntimeCall {
    RuntimeCall::new(
        "PolkadotXcm",
        "execute",
        vec![("message", withdraw_message(places, transfer)), ("max_weight", max_weight)],
    )
}

fn wild(name: &'static str, inner: Value) -> Value {
    Value::unnamed_variant("Wild", [Value::unnamed_variant(name, [inner])])
}

fn all_of(id: Value) -> Value {
    Value::unnamed_variant(
        "Wild",
        [Value::named_variant(
            "AllOf",
            [("id", id), ("fun", Value::unnamed_variant("Fungible", []))],
        )],
    )
}

/// The program Asset Hub runs on arrival: name the claimer of a trap, refund
/// the fee's surplus, sell all the CASH for at least the floor, deposit
/// everything on the landing account. Keyed as Asset Hub sees CASH.
fn remote_program(places: &super::Places, transfer: &Transfer) -> Vec<Value> {
    vec![
        Value::named_variant(
            "SetHints",
            [(
                "hints",
                Value::unnamed_composite([Value::named_variant(
                    "AssetClaimer",
                    [("location", beneficiary(&transfer.claimer))],
                )]),
            )],
        ),
        Value::unnamed_variant("RefundSurplus", []),
        Value::named_variant(
            "ExchangeAsset",
            [
                ("give", all_of(places.cash())),
                ("want", Value::unnamed_composite([places.asset(&native(), transfer.min_pas_out)])),
                ("maximal", Value::bool(true)),
            ],
        ),
        Value::named_variant(
            "DepositAsset",
            [
                ("assets", wild("AllCounted", Value::u128(2))),
                ("beneficiary", beneficiary(&transfer.landing)),
            ],
        ),
    ]
}

/// The XCM message: withdraw the PAS and all the CASH, pay People's fees
/// with the allowance, and send both to Asset Hub with the remote program.
/// The PAS always teleports; the CASH follows the network's transfer.
fn withdraw_message(places: &super::Places, transfer: &Transfer) -> Value {
    let cash = |amount| places.asset(&places.cash_on_people(), amount);
    let pas = |amount| places.asset(&native(), amount);
    let (remote_fees, filters) = match transfer.transfer {
        CashTransfer::Teleport => (
            "Teleport",
            vec![Value::unnamed_variant("Teleport", [wild("AllCounted", Value::u128(2))])],
        ),
        CashTransfer::Reserve => (
            "ReserveWithdraw",
            vec![
                Value::unnamed_variant("Teleport", [all_of(native())]),
                Value::unnamed_variant("ReserveWithdraw", [all_of(places.cash_on_people())]),
            ],
        ),
    };
    versioned(Value::unnamed_composite([
        Value::unnamed_variant(
            "WithdrawAsset",
            [Value::unnamed_composite([pas(transfer.pas_to_withdraw), cash(transfer.cash)])],
        ),
        Value::named_variant("PayFees", [("asset", pas(transfer.pay_fees_pas))]),
        Value::named_variant(
            "InitiateTransfer",
            [
                (
                    "destination",
                    location(1, vec![junction("Parachain", Value::u128(places.asset_hub_para.into()))]),
                ),
                (
                    "remote_fees",
                    Value::unnamed_variant(
                        "Some",
                        [Value::unnamed_variant(
                            remote_fees,
                            [Value::unnamed_variant(
                                "Definite",
                                [Value::unnamed_composite([cash(transfer.remote_fees_cash)])],
                            )],
                        )],
                    ),
                ),
                ("preserve_origin", Value::bool(false)),
                ("assets", Value::unnamed_composite(filters)),
                ("remote_xcm", Value::unnamed_composite(remote_program(places, transfer))),
            ],
        ),
    ]))
}

/// The message People forwards to Asset Hub for `transfer`, as the runtime
/// builds it, with the allowance's remainder as the PAS that travels: what
/// prices the delivery before a dry run produces the real one.
fn forwarded_stand_in(places: &super::Places, transfer: &Transfer) -> Value {
    let cash = |amount| places.asset(&places.cash_on_people(), amount);
    let pas = |amount| places.asset(&native(), amount);
    let pas_left = transfer.pas_to_withdraw.saturating_sub(transfer.pay_fees_pas);
    let fee = cash(transfer.remote_fees_cash);
    let mut message = match transfer.transfer {
        CashTransfer::Teleport => vec![
            Value::unnamed_variant("ReceiveTeleportedAsset", [Value::unnamed_composite([fee.clone()])]),
            Value::named_variant("PayFees", [("asset", fee)]),
            Value::unnamed_variant(
                "ReceiveTeleportedAsset",
                [Value::unnamed_composite(match pas_left {
                    0 => vec![cash(transfer.cash)],
                    left => vec![pas(left), cash(transfer.cash)],
                })],
            ),
        ],
        CashTransfer::Reserve => vec![
            Value::unnamed_variant("WithdrawAsset", [Value::unnamed_composite([fee.clone()])]),
            Value::named_variant("PayFees", [("asset", fee)]),
            Value::unnamed_variant(
                "ReceiveTeleportedAsset",
                [Value::unnamed_composite(match pas_left {
                    0 => Vec::new(),
                    left => vec![pas(left)],
                })],
            ),
            Value::unnamed_variant("WithdrawAsset", [Value::unnamed_composite([cash(transfer.cash)])]),
        ],
    };
    message.push(Value::unnamed_variant("ClearOrigin", []));
    message.extend(remote_program(places, transfer));
    message.push(Value::unnamed_variant("SetTopic", [Value::from_bytes([0; 32])]));
    versioned(Value::unnamed_composite(message))
}

/// The assets the events report trapped.
fn trapped_in(events: &Value) -> u128 {
    items(events)
        .into_iter()
        .filter_map(|event| find_variant(event, "AssetsTrapped"))
        .filter_map(|trap| field(trap, "assets").ok())
        .map(fungible_total)
        .sum()
}

/// The native token the events report credited to `account`.
fn credited_native(events: &Value, account: &[u8; 32]) -> u128 {
    items(events)
        .into_iter()
        .filter(|event| variant_name(event) == Some("Balances"))
        .filter_map(|event| {
            let credit = find_variant(event, "Minted").or_else(|| find_variant(event, "Deposit"))?;
            let who = account_bytes(field(credit, "who").ok()?)?;
            (who == *account).then(|| u128_at(credit, "amount").ok()).flatten()
        })
        .sum()
}

/// The 32 bytes of an `AccountId32` value.
fn account_bytes(value: &Value) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = items(unwrap_newtype(value))
        .into_iter()
        .filter_map(|byte| as_u128(byte).ok().and_then(|byte| u8::try_from(byte).ok()))
        .collect();
    bytes.try_into().ok()
}

/// The least a landing must reach to count: the dry run's, less the
/// slippage the program allows the sale, since a sale that slips further
/// fails the program and lands nothing.
pub fn landing_floor(expected: u128) -> u128 {
    expected - (expected / 100 * WITHDRAW_SLIPPAGE_PERCENT + expected % 100 * WITHDRAW_SLIPPAGE_PERCENT / 100)
}


#[cfg(test)]
mod live {
    //! Withdrawals sized against Paseo Next, from accounts that already hold
    //! CASH on People. Run with
    //! `cargo test -p truapi --lib funding::conversion::withdraw::live -- --ignored`.

    use subxt::config::substrate::SubstrateConfig;
    use subxt::dynamic;

    use super::*;
    use crate::runtime::FundingNetwork;
    use crate::runtime::funding::conversion::SigningChain;
    use crate::runtime::statement_allowance::{ChainClient, ChainContextCache};

    const ASSET_HUB: &str = "wss://paseo-asset-hub-next-rpc.polkadot.io";
    const PEOPLE: &str = "wss://paseo-people-next-system-rpc.polkadot.io";
    const NETWORK: FundingNetwork = FundingNetwork {
        cash_asset_id: 50_000_413,
    };

    async fn client(url: &str) -> subxt::OnlineClient<SubstrateConfig> {
        let rpc = subxt_rpcs::RpcClient::from_insecure_url(url)
            .await
            .expect("node reachable");
        let backend = subxt::backend::LegacyBackend::builder().build(rpc);
        subxt::OnlineClient::from_backend(std::sync::Arc::new(backend))
            .await
            .expect("client builds")
    }

    async fn chains() -> Chains {
        let people = client(PEOPLE).await;
        let rpc = crate::runtime::statement_allowance::rpc::RpcClient::connect(PEOPLE)
            .await
            .expect("node reachable");
        let context = ChainContextCache::default()
            .get(&ChainClient::new(rpc, people.genesis_hash().0))
            .await
            .expect("metadata");
        Chains::at_finalized(&client(ASSET_HUB).await, &people, NETWORK, Some(SigningChain::People(context)))
            .await
            .expect("chains pinned")
    }

    /// A People account holding at least `cash` CASH and `pas` PAS.
    async fn holder(chains: &Chains, cash: u128, pas: u128) -> [u8; 32] {
        let mut entries = chains
            .people
            .storage()
            .iter(
                dynamic::storage::<(Value, Value), Value>("Assets", "Account"),
                (chains.places.cash_on_people(),),
            )
            .await
            .expect("accounts iterate");
        while let Some(entry) = entries.next().await {
            let entry = entry.expect("entry reads");
            let balance = u128_at(&entry.value().decode().expect("decodes"), "balance").expect("balance");
            if balance < cash {
                continue;
            }
            let key = entry.key_bytes();
            let account: [u8; 32] = key[key.len() - 32..].try_into().expect("account id");
            let (_, held_pas) = chains.people_holdings(&account).await.expect("holdings");
            if held_pas >= pas {
                return account;
            }
        }
        panic!("no People account holds {cash} CASH and {pas} PAS");
    }

    fn quoting_key() -> schnorrkel::Keypair {
        schnorrkel::MiniSecretKey::from_bytes(&[1; 32])
            .expect("seed")
            .expand_to_keypair(schnorrkel::ExpansionMode::Ed25519)
    }

    async fn signing<'a>(chains: &'a Chains, signer: &'a Sr25519Signer, account: [u8; 32]) -> WithdrawSigning<'a> {
        WithdrawSigning {
            extensions: chains.people_extensions.as_deref().expect("People metadata"),
            signer,
            nonce: 0,
            account,
            landing: account,
            transfer: chains.cash_transfer().await.expect("transfer chosen"),
        }
    }

    // The swap is named and encoded as People's runtime takes it: a dry run
    // swaps the account's CASH for the PAS the XCM's fees need, and the cap
    // leaves CASH for the XCM.
    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn the_fee_swap_is_sized_from_a_live_dry_run() {
        let chains = chains().await;
        let account = holder(&chains, 2_000_000, 0).await;
        let (cash, _) = chains.people_holdings(&account).await.expect("holdings");
        let signer = Sr25519Signer::from_keypair(&quoting_key());

        let swap = chains
            .size_swap(&signing(&chains, &signer, account).await, cash)
            .await
            .expect("swap sized");

        assert_eq!(swap.call, WithdrawCall::Swap);
    }

    // The XCM's program is named and encoded as People's runtime takes it:
    // its fees are paid by an exact allowance, the forwarded program runs
    // on Asset Hub, sells the CASH and lands PAS on the landing account.
    #[tokio::test]
    #[ignore = "reaches Paseo Next"]
    async fn the_transfer_is_sized_from_live_dry_runs_on_both_chains() {
        let chains = chains().await;
        let account = holder(&chains, 2_000_000, 20_000_000_000).await;
        let (cash, pas) = chains.people_holdings(&account).await.expect("holdings");
        let signer = Sr25519Signer::from_keypair(&quoting_key());

        let transfer = match chains.size_transfer(&signing(&chains, &signer, account).await, cash, pas).await {
            Ok(prepared) => prepared,
            Err(Sizing::NeedsSwap) => panic!("the holder lacks the PAS for the fees"),
            Err(Sizing::Failed(error)) => panic!("transfer not sized: {error}"),
        };

        assert!(matches!(transfer.call, WithdrawCall::Transfer { expected_landing } if expected_landing > 0));
    }
}
