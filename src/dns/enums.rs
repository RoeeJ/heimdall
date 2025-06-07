#[derive(Copy, Clone, Debug, Default)]
pub enum DNSResourceType {
    #[default]
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
}

#[derive(Copy, Clone, Debug, Default)]
pub enum DNSResourceClass {
    #[default]
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
            _ => DNSResourceClass::IN,
        }
    }
}

impl Into<u16> for DNSResourceClass {
    fn into(self) -> u16 {
        match self {
            DNSResourceClass::IN => 1,
            DNSResourceClass::CS => 2,
            DNSResourceClass::CH => 3,
            DNSResourceClass::HS => 4,
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
            257 => DNSResourceType::CAA,
            
            x => {
                eprintln!("Unrecognized type: {}", x);
                return DNSResourceType::default();
            }
        }
    }
}

impl Into<u16> for DNSResourceType {
    fn into(self) -> u16 {
        match self {
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
        }
    }
}
