const std = @import("std");

pub const DNSQueryType = enum(u16) {
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
};

pub const DNSClassType = enum(u16) {
    IN = 1,
    CS = 2,
    CH = 3,
    HS = 4,
    ANY = 255,
};

pub const DNSResponseCode = enum(u4) {
    NO_ERROR = 0,
    INVALID_FORMAT = 1,
    SERVER_ERROR = 2,
    NAME_ERROR = 3,
    REQUEST_NOT_SUPPORTED = 4,
    POLICY_FAIL = 5,
    UNK,
};

pub const DNSOpcode = enum(u4) {
    QUERY = 0,
    IQUERY = 1,
    STATUS = 2,
    NOTIFY = 4,
    UPDATE = 5,
    STATEFULUPDATE = 6,
    UNK,
};

pub fn parse_qtype(b: u16) DNSQueryType {
    return std.meta.intToEnum(DNSQueryType, b) catch {
        return .UNK;
    };
}

pub fn parse_rcode(b: u4) DNSResponseCode {
    return std.meta.intToEnum(DNSResponseCode, b) catch {
        return .UNK;
    };
}

pub fn parse_opcode(b: u4) DNSOpcode {
    return std.meta.intToEnum(DNSOpcode, b) catch {
        return .UNK;
    };
}

pub fn parse_classtype(b: u16) DNSClassType {
    return std.meta.intToEnum(DNSClassType, b) catch |ex| {
        std.log.info("{}/{}", .{ b, ex });
        return .ANY;
    };
}
