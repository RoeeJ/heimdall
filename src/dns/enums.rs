#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum DNSResourceType {
    #[default]
    Unknown,
    A,
    NS,
    MD,
    MF,
    CNAME,
    SOA,
    PTR,
    HINFO,
    MX,
    TXT,
    AAAA,
    AXFR,
    MAILB,
    // Additional common types
    SRV,
    SSHFP,
    TLSA,
    HTTPS,
    CAA,
    DS,
    DNSKEY,
    NSEC,
    RRSIG,
    OPT,
    ANY,
    IXFR,

    // Phase 1: Core Record Types
    // Location & Service Discovery
    LOC,   // Location information (RFC 1876)
    NAPTR, // Naming Authority Pointer (RFC 2915)
    APL,   // Address Prefix List (RFC 3123)

    // Mail & Communication
    SPF, // Sender Policy Framework (RFC 7208)

    // Security Extensions
    NSEC3,      // Next Secure v3 (RFC 5155)
    NSEC3PARAM, // NSEC3 parameters (RFC 5155)
    CDNSKEY,    // Child DNSKEY (RFC 7344)
    CDS,        // Child DS (RFC 7344)

    // Modern Web
    SVCB,   // Service Binding (RFC 9460)
    SMIMEA, // S/MIME cert association (RFC 8162)

    // Experimental/Legacy
    RP,    // Responsible Person (RFC 1183)
    AFSDB, // AFS Database (RFC 1183)

    // Additional Essential Types
    DNAME, // Domain Name redirection (RFC 6672)
    URI,   // Uniform Resource Identifier (RFC 7553)

    // Phase 2: DNSSEC & Security Types
    // DNSSEC Core
    KEY,      // Security key (RFC 2065) - legacy
    SIG,      // Security signature (RFC 2065) - legacy
    NXT,      // Next domain (RFC 2065) - legacy, replaced by NSEC
    DHCID,    // DHCP identifier (RFC 4701)
    IPSECKEY, // IPsec key (RFC 4025)
    HIP,      // Host Identity Protocol (RFC 8005)

    // Trust & Validation
    CSYNC,      // Child-to-parent synchronization (RFC 7477)
    ZONEMD,     // Message digest for DNS zone (RFC 8976)
    OPENPGPKEY, // OpenPGP public key (RFC 7929)

    // Certificate Management
    CERT, // Certificate record (RFC 4398)
    KX,   // Key Exchange (RFC 2230)
    TKEY, // Transaction Key (RFC 2930)

    // Phase 3: Network & Infrastructure Types
    // Network Infrastructure
    WKS,     // Well Known Services (RFC 1035)
    X25,     // X.25 PSDN address (RFC 1183)
    ISDN,    // ISDN address (RFC 1183)
    RT,      // Route Through (RFC 1183)
    NSAP,    // Network Service Access Point (RFC 1706)
    NSAPPTR, // NSAP pointer (RFC 1706)
    PX,      // X.400 mail mapping (RFC 2163)
    GPOS,    // Geographical Position (RFC 1712) - obsolete

    // Addressing Extensions
    A6,     // IPv6 address (RFC 3226) - obsolete, use AAAA
    ATMA,   // ATM Address (AF/BF)
    EID,    // Endpoint Identifier (RFC 7598)
    NIMLOC, // Nimrod Locator (RFC 1712)
    L32,    // 32-bit Locator (RFC 7598)
    L64,    // 64-bit Locator (RFC 7598)
    LP,     // Locator Pointer (RFC 7598)

    // Hardware Identifiers
    EUI48, // 48-bit Extended Unique Identifier (RFC 7043)
    EUI64, // 64-bit Extended Unique Identifier (RFC 7043)
    NID,   // Node Identifier (RFC 6742)

    // Phase 4: Advanced & Future Types
    // Experimental/Research
    SINK,   // Application sink (RFC 7598)
    NINFO,  // Zone status info (Jim Reid)
    RKEY,   // Resource key
    TALINK, // Trust Anchor Link
    NULL,   // Null record (RFC 1035)

    // Zone Management
    TSIG,  // Transaction Signature (RFC 8945)
    MINFO, // Mailbox info (RFC 1035)
    MB,    // Mailbox (RFC 1035)
    MG,    // Mail group (RFC 1035)
    MR,    // Mail rename (RFC 1035)

    // Additional Types
    TA,  // DNSSEC Trust Authorities (RFC 8310)
    DLV, // DNSSEC Lookaside Validation (RFC 4431)

    // Enum for generic/unknown types
    UNSPEC, // Unspecified (RFC 1035)
    UINFO,  // User info (Draft)
    UID,    // User ID (Draft)
    GID,    // Group ID (Draft)
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[rkyv(derive(Debug, PartialEq))]
pub enum DNSResourceClass {
    #[default]
    Unknown,
    IN,
    CS,
    CH,
    HS,
}

impl From<u16> for DNSResourceClass {
    fn from(value: u16) -> Self {
        match value {
            1 => DNSResourceClass::IN,
            2 => DNSResourceClass::CS,
            3 => DNSResourceClass::CH,
            4 => DNSResourceClass::HS,
            _ => DNSResourceClass::Unknown,
        }
    }
}

impl From<DNSResourceClass> for u16 {
    fn from(value: DNSResourceClass) -> Self {
        match value {
            DNSResourceClass::IN => 1,
            DNSResourceClass::CS => 2,
            DNSResourceClass::CH => 3,
            DNSResourceClass::HS => 4,
            DNSResourceClass::Unknown => 0,
        }
    }
}

impl From<u16> for DNSResourceType {
    fn from(value: u16) -> Self {
        match value {
            1 => DNSResourceType::A,
            2 => DNSResourceType::NS,
            3 => DNSResourceType::MD,
            4 => DNSResourceType::MF,
            5 => DNSResourceType::CNAME,
            6 => DNSResourceType::SOA,
            12 => DNSResourceType::PTR,
            13 => DNSResourceType::HINFO,
            15 => DNSResourceType::MX,
            16 => DNSResourceType::TXT,
            28 => DNSResourceType::AAAA,
            33 => DNSResourceType::SRV,
            41 => DNSResourceType::OPT,
            43 => DNSResourceType::DS,
            44 => DNSResourceType::SSHFP,
            46 => DNSResourceType::RRSIG,
            47 => DNSResourceType::NSEC,
            48 => DNSResourceType::DNSKEY,
            52 => DNSResourceType::TLSA,
            65 => DNSResourceType::HTTPS,
            252 => DNSResourceType::AXFR,
            253 => DNSResourceType::MAILB,
            254 => DNSResourceType::IXFR,
            255 => DNSResourceType::ANY,
            257 => DNSResourceType::CAA,

            // Phase 1 type mappings
            29 => DNSResourceType::LOC,
            35 => DNSResourceType::NAPTR,
            42 => DNSResourceType::APL,
            99 => DNSResourceType::SPF,
            50 => DNSResourceType::NSEC3,
            51 => DNSResourceType::NSEC3PARAM,
            60 => DNSResourceType::CDNSKEY,
            59 => DNSResourceType::CDS,
            64 => DNSResourceType::SVCB,
            53 => DNSResourceType::SMIMEA,
            17 => DNSResourceType::RP,
            18 => DNSResourceType::AFSDB,
            39 => DNSResourceType::DNAME,
            256 => DNSResourceType::URI,

            // Phase 2 type mappings - DNSSEC & Security
            25 => DNSResourceType::KEY,
            24 => DNSResourceType::SIG,
            30 => DNSResourceType::NXT,
            49 => DNSResourceType::DHCID,
            45 => DNSResourceType::IPSECKEY,
            55 => DNSResourceType::HIP,
            62 => DNSResourceType::CSYNC,
            63 => DNSResourceType::ZONEMD,
            61 => DNSResourceType::OPENPGPKEY,
            37 => DNSResourceType::CERT,
            36 => DNSResourceType::KX,
            249 => DNSResourceType::TKEY,

            // Phase 3 type mappings - Network & Infrastructure
            11 => DNSResourceType::WKS,
            19 => DNSResourceType::X25,
            20 => DNSResourceType::ISDN,
            21 => DNSResourceType::RT,
            22 => DNSResourceType::NSAP,
            23 => DNSResourceType::NSAPPTR,
            26 => DNSResourceType::PX,
            27 => DNSResourceType::GPOS,
            38 => DNSResourceType::A6,
            34 => DNSResourceType::ATMA,
            31 => DNSResourceType::EID,
            32 => DNSResourceType::NIMLOC,
            105 => DNSResourceType::L32,
            106 => DNSResourceType::L64,
            107 => DNSResourceType::LP,
            108 => DNSResourceType::EUI48,
            109 => DNSResourceType::EUI64,
            104 => DNSResourceType::NID,

            // Phase 4 type mappings - Advanced & Future Types
            40 => DNSResourceType::SINK,
            56 => DNSResourceType::NINFO,
            57 => DNSResourceType::RKEY,
            58 => DNSResourceType::TALINK,
            10 => DNSResourceType::NULL,
            250 => DNSResourceType::TSIG,
            14 => DNSResourceType::MINFO,
            7 => DNSResourceType::MB,
            8 => DNSResourceType::MG,
            9 => DNSResourceType::MR,
            32768 => DNSResourceType::TA,
            32769 => DNSResourceType::DLV,
            103 => DNSResourceType::UNSPEC,
            100 => DNSResourceType::UINFO,
            101 => DNSResourceType::UID,
            102 => DNSResourceType::GID,

            _ => DNSResourceType::Unknown,
        }
    }
}

impl From<DNSResourceType> for u16 {
    fn from(value: DNSResourceType) -> Self {
        match value {
            DNSResourceType::A => 1,
            DNSResourceType::NS => 2,
            DNSResourceType::MD => 3,
            DNSResourceType::MF => 4,
            DNSResourceType::CNAME => 5,
            DNSResourceType::SOA => 6,
            DNSResourceType::PTR => 12,
            DNSResourceType::HINFO => 13,
            DNSResourceType::MX => 15,
            DNSResourceType::TXT => 16,
            DNSResourceType::AAAA => 28,
            DNSResourceType::SRV => 33,
            DNSResourceType::SSHFP => 44,
            DNSResourceType::RRSIG => 46,
            DNSResourceType::NSEC => 47,
            DNSResourceType::DNSKEY => 48,
            DNSResourceType::DS => 43,
            DNSResourceType::TLSA => 52,
            DNSResourceType::HTTPS => 65,
            DNSResourceType::CAA => 257,
            DNSResourceType::OPT => 41,
            DNSResourceType::AXFR => 252,
            DNSResourceType::MAILB => 253,
            DNSResourceType::IXFR => 254,
            DNSResourceType::ANY => 255,

            // Phase 1 type mappings
            DNSResourceType::LOC => 29,
            DNSResourceType::NAPTR => 35,
            DNSResourceType::APL => 42,
            DNSResourceType::SPF => 99,
            DNSResourceType::NSEC3 => 50,
            DNSResourceType::NSEC3PARAM => 51,
            DNSResourceType::CDNSKEY => 60,
            DNSResourceType::CDS => 59,
            DNSResourceType::SVCB => 64,
            DNSResourceType::SMIMEA => 53,
            DNSResourceType::RP => 17,
            DNSResourceType::AFSDB => 18,
            DNSResourceType::DNAME => 39,
            DNSResourceType::URI => 256,

            // Phase 2 type mappings - DNSSEC & Security
            DNSResourceType::KEY => 25,
            DNSResourceType::SIG => 24,
            DNSResourceType::NXT => 30,
            DNSResourceType::DHCID => 49,
            DNSResourceType::IPSECKEY => 45,
            DNSResourceType::HIP => 55,
            DNSResourceType::CSYNC => 62,
            DNSResourceType::ZONEMD => 63,
            DNSResourceType::OPENPGPKEY => 61,
            DNSResourceType::CERT => 37,
            DNSResourceType::KX => 36,
            DNSResourceType::TKEY => 249,

            // Phase 3 type mappings - Network & Infrastructure
            DNSResourceType::WKS => 11,
            DNSResourceType::X25 => 19,
            DNSResourceType::ISDN => 20,
            DNSResourceType::RT => 21,
            DNSResourceType::NSAP => 22,
            DNSResourceType::NSAPPTR => 23,
            DNSResourceType::PX => 26,
            DNSResourceType::GPOS => 27,
            DNSResourceType::A6 => 38,
            DNSResourceType::ATMA => 34,
            DNSResourceType::EID => 31,
            DNSResourceType::NIMLOC => 32,
            DNSResourceType::L32 => 105,
            DNSResourceType::L64 => 106,
            DNSResourceType::LP => 107,
            DNSResourceType::EUI48 => 108,
            DNSResourceType::EUI64 => 109,
            DNSResourceType::NID => 104,

            // Phase 4 type mappings - Advanced & Future Types
            DNSResourceType::SINK => 40,
            DNSResourceType::NINFO => 56,
            DNSResourceType::RKEY => 57,
            DNSResourceType::TALINK => 58,
            DNSResourceType::NULL => 10,
            DNSResourceType::TSIG => 250,
            DNSResourceType::MINFO => 14,
            DNSResourceType::MB => 7,
            DNSResourceType::MG => 8,
            DNSResourceType::MR => 9,
            DNSResourceType::TA => 32768,
            DNSResourceType::DLV => 32769,
            DNSResourceType::UNSPEC => 103,
            DNSResourceType::UINFO => 100,
            DNSResourceType::UID => 101,
            DNSResourceType::GID => 102,

            DNSResourceType::Unknown => 0,
        }
    }
}

/// DNS Response Codes (RCODEs) as defined in RFC 1035 and subsequent RFCs
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResponseCode {
    /// No error condition
    NoError = 0,
    /// Format error - The name server was unable to interpret the query
    FormatError = 1,
    /// Server failure - The name server was unable to process due to a problem with the name server
    ServerFailure = 2,
    /// Name Error - Domain name referenced in the query does not exist
    NameError = 3, // NXDOMAIN
    /// Not Implemented - The name server does not support the requested kind of query
    NotImplemented = 4,
    /// Refused - The name server refuses to perform the specified operation for policy reasons
    Refused = 5,
    /// YXDomain - Name exists when it should not (RFC 2136)
    YXDomain = 6,
    /// YXRRSet - RR Set exists when it should not (RFC 2136)
    YXRRSet = 7,
    /// NXRRSet - RR Set that should exist does not (RFC 2136)
    NXRRSet = 8,
    /// NotAuth - Server is not authoritative for zone (RFC 2136)
    NotAuth = 9,
    /// NotZone - Name not contained in zone (RFC 2136)
    NotZone = 10,
    /// BadOptVersion - Bad OPT Version (RFC 6891)
    BadOptVersion = 16,
}

impl ResponseCode {
    /// Convert RCODE to u8 for DNS packet encoding
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Convert u8 to RCODE, defaulting to ServerFailure for unknown codes
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ResponseCode::NoError,
            1 => ResponseCode::FormatError,
            2 => ResponseCode::ServerFailure,
            3 => ResponseCode::NameError,
            4 => ResponseCode::NotImplemented,
            5 => ResponseCode::Refused,
            6 => ResponseCode::YXDomain,
            7 => ResponseCode::YXRRSet,
            8 => ResponseCode::NXRRSet,
            9 => ResponseCode::NotAuth,
            10 => ResponseCode::NotZone,
            16 => ResponseCode::BadOptVersion,
            _ => ResponseCode::ServerFailure, // Default to SERVFAIL for unknown codes
        }
    }

    /// Check if this is a successful response code
    pub fn is_success(self) -> bool {
        matches!(self, ResponseCode::NoError)
    }

    /// Check if this is an error that should be cached (negative caching)
    pub fn is_cacheable_error(self) -> bool {
        matches!(self, ResponseCode::NameError) // NXDOMAIN responses are cached
    }

    /// Get human-readable description of the response code
    pub fn description(self) -> &'static str {
        match self {
            ResponseCode::NoError => "No error",
            ResponseCode::FormatError => "Format error",
            ResponseCode::ServerFailure => "Server failure",
            ResponseCode::NameError => "Name error (NXDOMAIN)",
            ResponseCode::NotImplemented => "Not implemented",
            ResponseCode::Refused => "Refused",
            ResponseCode::YXDomain => "Name exists when it should not",
            ResponseCode::YXRRSet => "RR Set exists when it should not",
            ResponseCode::NXRRSet => "RR Set that should exist does not",
            ResponseCode::NotAuth => "Server not authoritative for zone",
            ResponseCode::NotZone => "Name not contained in zone",
            ResponseCode::BadOptVersion => "Bad OPT Version",
        }
    }
}

/// DNS Opcodes as defined in RFC 1035 and subsequent RFCs
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DnsOpcode {
    /// Query - Standard DNS query (RFC 1035)
    Query = 0,
    /// Inverse Query - Obsolete (RFC 3425)
    IQuery = 1,
    /// Status - Server status request (RFC 1035)
    Status = 2,
    /// Unassigned
    Unassigned3 = 3,
    /// Notify - Zone change notification (RFC 1996)
    Notify = 4,
    /// Update - Dynamic DNS update (RFC 2136)
    Update = 5,
    /// DNS Stateful Operations (RFC 8490)
    DSO = 6,
}

impl DnsOpcode {
    /// Convert opcode to u8 for DNS packet encoding
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Convert u8 to opcode
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(DnsOpcode::Query),
            1 => Some(DnsOpcode::IQuery),
            2 => Some(DnsOpcode::Status),
            3 => Some(DnsOpcode::Unassigned3),
            4 => Some(DnsOpcode::Notify),
            5 => Some(DnsOpcode::Update),
            6 => Some(DnsOpcode::DSO),
            _ => None, // Values 7-15 are unassigned
        }
    }

    /// Check if this opcode is implemented
    pub fn is_implemented(self) -> bool {
        matches!(self, DnsOpcode::Query) // Only QUERY is currently implemented
    }

    /// Get human-readable description of the opcode
    pub fn description(self) -> &'static str {
        match self {
            DnsOpcode::Query => "Standard query",
            DnsOpcode::IQuery => "Inverse query (obsolete)",
            DnsOpcode::Status => "Server status request",
            DnsOpcode::Unassigned3 => "Unassigned",
            DnsOpcode::Notify => "Zone change notification",
            DnsOpcode::Update => "Dynamic DNS update",
            DnsOpcode::DSO => "DNS Stateful Operations",
        }
    }
}
