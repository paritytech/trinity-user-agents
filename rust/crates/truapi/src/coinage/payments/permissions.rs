//! J1 · Purse ownership and permissions: what one call needs from the user
//! before it may go ahead.
//!
//! The creator of a purse is its layer-1 tag (invariant 2), and every answer
//! the user gives is stored by the core's permission service, which asks
//! once and shows each grant in the host's settings. No product reaches a
//! purse without RFC-17's `CoinPayment` permission, except an RFC-6 call on
//! the main purse (invariant 5).

use truapi::latest::{CoinPaymentPurseAction, RemotePermission};

use crate::coinage::PurseId;

/// What a call does with a purse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
	/// Read its metadata and balance.
	Query,
	/// Let value land in it.
	Receive,
	/// Let value leave it.
	Spend,
	/// Add value from outside coinage.
	TopUp,
}

/// Which surface a call came through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
	/// RFC-17.
	Rfc17,
	/// RFC-6, which needs no `CoinPayment` permission on the main purse.
	Rfc6,
}

/// One thing a call needs from the user, in the order they are asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Requirement {
	/// A stored permission: the permission service asks once and remembers
	/// the answer until the user revokes it.
	Permission(RemotePermission),
	/// The user's confirmation of this one spend (invariant 4).
	ConfirmSpend,
}

/// What `product` needs before it may do `action` on `purse`, whose layer-1
/// tag names its `creator`:
/// 1. the `CoinPayment` permission, except for RFC-6 on the main purse;
/// 2. nothing more for a top-up (invariant 3), or a query or a receive on a
///    purse the product created (invariant 1);
/// 3. RFC-6's balance access for an RFC-6 query of the main purse, the grant
///    the payment capability asks for first, so the user is asked once
///    (invariant 6);
/// 4. otherwise the permission for that purse and action;
/// 5. and, for a spend, the user's confirmation (invariant 4).
pub fn requirements(
	product: &[u8],
	purse: PurseId,
	creator: Option<&[u8]>,
	action: Action,
	surface: Surface,
) -> Vec<Requirement> {
	let mut needed = Vec::new();
	if !(surface == Surface::Rfc6 && purse == PurseId::MAIN) {
		needed.push(Requirement::Permission(RemotePermission::CoinPayment));
	}
	let purse_action = match action {
		Action::TopUp => return needed,
		Action::Query => CoinPaymentPurseAction::Query,
		Action::Receive => CoinPaymentPurseAction::Receive,
		Action::Spend => CoinPaymentPurseAction::Spend,
	};
	if surface == Surface::Rfc6 && purse == PurseId::MAIN && action == Action::Query {
		needed.push(Requirement::Permission(RemotePermission::BalanceAccess));
	} else if creator != Some(product) {
		needed.push(Requirement::Permission(RemotePermission::CoinPaymentPurse { purse: purse.0, action: purse_action }));
	}
	if action == Action::Spend {
		needed.push(Requirement::ConfirmSpend);
	}
	needed
}

#[cfg(test)]
mod tests;
