//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, so a host that has not started them over one, such as the browser
//! core, answers `Unsupported` to the served calls.

mod served;
