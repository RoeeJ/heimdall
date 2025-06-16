/// DNS Response Code constants from RFC 1035 and subsequent RFCs
pub struct DNSRcode;

impl DNSRcode {
    pub const NOERROR: u8 = 0; // No error
    pub const FORMERR: u8 = 1; // Format error
    pub const SERVFAIL: u8 = 2; // Server failure
    pub const NXDOMAIN: u8 = 3; // Name error
    pub const NOTIMP: u8 = 4; // Not implemented
    pub const REFUSED: u8 = 5; // Query refused
    pub const YXDOMAIN: u8 = 6; // Name exists when it should not
    pub const YXRRSET: u8 = 7; // RR Set exists when it should not
    pub const NXRRSET: u8 = 8; // RR Set that should exist does not
    pub const NOTAUTH: u8 = 9; // Not authorized
    pub const NOTZONE: u8 = 10; // Name not contained in zone
    pub const BADVERS: u8 = 16; // Bad OPT version
}

/// DNS Opcode constants from RFC 1035
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Opcode {
    QUERY = 0,
    IQUERY = 1,
    STATUS = 2,
    UNASSIGNED3 = 3,
    NOTIFY = 4,
    UPDATE = 5,
    DSO = 6,
}

impl From<u8> for Opcode {
    fn from(value: u8) -> Self {
        match value {
            0 => Opcode::QUERY,
            1 => Opcode::IQUERY,
            2 => Opcode::STATUS,
            3 => Opcode::UNASSIGNED3,
            4 => Opcode::NOTIFY,
            5 => Opcode::UPDATE,
            6 => Opcode::DSO,
            _ => Opcode::QUERY, // Default to QUERY for unknown values
        }
    }
}
