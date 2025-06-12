pub mod errors;
pub mod parser;
pub mod record;
pub mod store;
#[allow(clippy::module_inception)]
pub mod zone;

pub use errors::{Result, ZoneError};
pub use parser::ZoneParser;
pub use record::ZoneRecord;
pub use store::{QueryResult, ZoneStore};
pub use zone::Zone;

/// Zone constants
pub mod constants {
    /// Default SOA refresh interval (24 hours)
    pub const DEFAULT_SOA_REFRESH: u32 = 86400;

    /// Default SOA retry interval (2 hours)
    pub const DEFAULT_SOA_RETRY: u32 = 7200;

    /// Default SOA expire interval (1 week)
    pub const DEFAULT_SOA_EXPIRE: u32 = 604800;

    /// Default SOA minimum TTL (1 hour)
    pub const DEFAULT_SOA_MINIMUM: u32 = 3600;

    /// Default TTL if not specified (1 hour)
    pub const DEFAULT_TTL: u32 = 3600;

    /// Maximum zone file size (10MB)
    pub const MAX_ZONE_FILE_SIZE: usize = 10 * 1024 * 1024;
}
