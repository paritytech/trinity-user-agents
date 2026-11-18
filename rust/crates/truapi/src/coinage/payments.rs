//! Layer 2, coinage payments: the RFC-6 and RFC-17 interfaces products use,
//! built on the coinage layer, from `docs/design/coinage/layer-2-components.md`.
//!
//! Each public submodule is one component of that document and names it in
//! its documentation.

pub mod cheque;
pub mod clearing;
pub mod receivable;
pub mod records;
pub mod status;
pub mod topic;
