pub const PORT: u16 = 1053;
pub const MAX_UDP_PACKET_SIZE: usize = 512;
pub const EDNS_VERSION: u8 = 0;
pub const EDNS_UDP_SIZE: u16 = 4096;
pub const FORWARD_DNS_SERVER: &str = "1.1.1.1";
pub const SERVER_COOKIE: [u8; 8] = [0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe];

// Blocklist configuration
pub const BLOCKLIST_URL: &str = "https://cdn.jsdelivr.net/gh/hagezi/dns-blocklists@latest/hosts/pro.plus.txt";
pub const BLOCKLIST_REFRESH_INTERVAL: u64 = 86400; // 24 hours in seconds

// Public Suffix List URL
pub const PUBLIC_SUFFIX_LIST_URL: &str = "https://publicsuffix.org/list/public_suffix_list.dat";
