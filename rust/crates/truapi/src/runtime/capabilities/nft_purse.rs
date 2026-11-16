//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, so a host that configured none answers `Unsupported` to the
//! served calls, as the browser core does today.

mod served;
