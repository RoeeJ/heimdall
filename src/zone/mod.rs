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
    /// Default TTL if not specified (1 hour)
    pub const DEFAULT_TTL: u32 = 3600;

    /// Maximum zone file size (10MB)
    pub const MAX_ZONE_FILE_SIZE: usize = 10 * 1024 * 1024;
}
