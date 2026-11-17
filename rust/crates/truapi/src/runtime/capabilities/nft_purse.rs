//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, so a host that configured none, such as the browser core,
//! answers `Unsupported` to the served calls.

mod served;
