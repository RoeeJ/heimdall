use num_traits::{FromPrimitive, ToPrimitive};
use ux::{u3, u4};

#[derive(Default, Debug, Clone)]
pub struct Packet {
    pub id: u16,
    pub qr: bool,
    pub op: Opcode,
    pub aa: bool,
    pub tc: bool,
    pub rd: bool,
    pub ra: bool,
    pub z: u3,
    pub rcode: RCode,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
    pub questions: Vec<Question>,
    pub answers: Vec<Resource>,
    pub name_servers: Vec<Resource>,
    pub resources: Vec<Resource>,
}

#[derive(Default, Debug, Clone)]
pub struct Question {
    pub qname: Vec<String>,
    pub qtype: QueryType,
    pub qclass: QueryClass,
}

#[derive(Default, Debug, Clone)]
pub struct Answer {
    pub name: Name,
}

#[derive(Default, Debug, Clone)]
pub enum Name {
    Pointer(u8),
    String(Vec<String>),
    Root,
    #[default]
    Empty,
}

#[derive(FromPrimitive, Default, Debug, Copy, Clone, PartialEq)]
pub enum QueryType {
    #[default]
    UNK = 0,
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    HINFO = 13,
    MX = 15,
    TXT = 16,
    RP = 17,
    AFSDB = 18,
    SIG = 24,
    KEY = 25,
    AAAA = 28,
    LOC = 29,
    NAPTR = 35,
    KX = 36,
    CERT = 37,
    DNAME = 39,
    OPT = 41,
    APL = 42,
    DS = 43,
    SSHFP = 44,
    IPSECKEY = 45,
    RRSIG = 46,
    NSEC = 47,
    DNSKEY = 48,
    DHCID = 49,
    NSEC3 = 50,
    NSEC3PARAM = 51,
    TLSA = 52,
    SMIMEA = 53,
    HIP = 55,
    CDS = 59,
    CDNSKEY = 60,
    OPENPGPKEY = 61,
    CSYNC = 62,
    HTTPS = 65,
    ZONEMD = 63,
    SVCB = 64,
    EUI48 = 108,
    EUI64 = 109,
    TKEY = 249,
    TSIG = 250,
    AXFR = 252,
    MAILB = 253,
    MAILA = 254,
    ALL = 255,
    URI = 256,
    CAA = 257,
    TA = 32768,
    DLV = 32769,
}

#[derive(Default, Debug, Copy, Clone)]
#[repr(u16)]
pub enum QueryClass {
    #[default]
    IN = 1,
    CS = 2,
    CH = 3,
    HS = 4,
    ANY = 255,
    Other(u16),
}

impl ToPrimitive for QueryClass {
    fn to_i64(&self) -> Option<i64> {
        Some(match self {
            QueryClass::IN => 1,
            QueryClass::CS => 2,
            QueryClass::CH => 3,
            QueryClass::HS => 4,
            QueryClass::ANY => 255,
            QueryClass::Other(x) => *x as i64,
        })
    }

    fn to_u64(&self) -> Option<u64> {
        Some(match self {
            QueryClass::IN => 1,
            QueryClass::CS => 2,
            QueryClass::CH => 3,
            QueryClass::HS => 4,
            QueryClass::ANY => 255,
            QueryClass::Other(x) => *x as u64,
        })
    }
}

impl FromPrimitive for QueryClass {
    fn from_i64(n: i64) -> Option<Self> {
        Some(match n {
            1 => Self::IN,
            2 => Self::CS,
            3 => Self::CH,
            4 => Self::HS,
            255 => Self::ANY,
            x => Self::Other(x as u16),
        })
    }

    fn from_u64(n: u64) -> Option<Self> {
        Some(match n {
            1 => Self::IN,
            2 => Self::CS,
            3 => Self::CH,
            4 => Self::HS,
            255 => Self::ANY,
            x => Self::Other(x as u16),
        })
    }
}

#[derive(Default, Debug, Clone)]
pub struct Resource {
    pub name: Name,
    pub qtype: QueryType,
    pub qclass: QueryClass,
    pub ttl: u32,
    pub data: Vec<u8>,
}

#[derive(FromPrimitive, Default, Debug, Clone, Copy)]
#[repr(u8)]
pub enum Opcode {
    #[default]
    Query = 0,
    IQuery = 1,
    Status = 2,
    Notify = 4,
    Update = 5,
    StatefulUpdate = 6,
    Other,
}

#[derive(FromPrimitive, Default, Debug, Clone, Copy)]
#[repr(u8)]
pub enum RCode {
    #[default]
    NoError = 0,
    InvalidFormat = 1,
    ServerError = 2,
    NameError = 3,
    RequestNotSupported = 4,
    PolicyFail = 5,
    UNK,
}
