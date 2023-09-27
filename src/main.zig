const std = @import("std");
const network = @import("network");
const print = @import("std").debug.print;
const enums = @import("enums.zig");

const allocator = std.heap.c_allocator;
const DUMP_PACKET: bool = true;
var counter = @as(i32, 0);
const DNSPacket = struct {
    ID: u16, // ID
    QR: u1, // 0 for Query, 1 for Response
    OP: enums.DNSOpcode,
    AA: u1, // Authoritative Answer
    TC: u1, // Truncated
    RD: u1, // Recursion Desired
    RA: u1, // Recursion Available
    Z: u3, // Unused
    RCODE: enums.DNSResponseCode,
    QDCOUNT: u16,
    ANCOUNT: u16,
    NSCOUNT: u16,
    ARCOUNT: u16,
    Questions: []DNSQuestion,
    Answers: []Answer,
    pub fn toBytes(packet: DNSPacket) ![]u8 {
        var data = std.ArrayList(u8).init(allocator);
        var bw = std.io.bitWriter(.Big, data.writer());
        try bw.writeBits(packet.ID, 16);
        try bw.writeBits(packet.QR, 1);
        try bw.writeBits(@intFromEnum(packet.OP), 4);
        try bw.writeBits(packet.AA, 1);
        try bw.writeBits(packet.TC, 1);
        try bw.writeBits(packet.RD, 1);
        try bw.writeBits(packet.RA, 1);
        try bw.writeBits(packet.Z, 3);
        try bw.writeBits(@intFromEnum(packet.RCODE), 4);
        try bw.writeBits(packet.QDCOUNT, 16);
        try bw.writeBits(packet.ANCOUNT, 16);
        try bw.writeBits(packet.NSCOUNT, 16);
        try bw.writeBits(packet.ARCOUNT, 16);

        var cq: usize = 0;
        while (cq < packet.Questions.len and packet.Questions.len != 0) : (cq += 1) {
            var cqq = packet.Questions[cq];
            try writeQuestion(cqq, &data);
        }

        var ca: usize = 0;
        while (ca < packet.Answers.len and packet.Answers.len != 0) : (ca += 1) {
            var caa = packet.Answers[ca];
            try writeAnswer(caa, &data);
        }

        return data.items;
    }
};

fn writeQuestion(q: DNSQuestion, data: *std.ArrayList(u8)) !void {
    var bw = std.io.bitWriter(.Big, data.writer());
    try write_qname(data, q.QNAME);
    _ = try bw.writeBits(@as(u16, @intFromEnum(q.QTYPE)), 16);
    _ = try bw.writeBits(@as(u16, @intFromEnum(q.QCLASS)), 16);
}

fn writeAnswer(res: Answer, data: *std.ArrayList(u8)) !void {
    var bw = std.io.bitWriter(.Big, data.writer());

    if (res.QNAME.len == 0) {
        std.log.info("Empty name", .{});
    } else {
        try write_qname(data, res.QNAME);
    }
    try bw.writeBits(@as(u16, @intFromEnum(res.QTYPE)), 16);
    try bw.writeBits(@as(u16, @intFromEnum(res.QCLASS)), 16);
    try bw.writeBits(@as(u32, @truncate(res.TTL)), 32);
    try bw.writeBits(@as(u16, res.DATA_LENGTH), 16);
    _ = try bw.write(res.DATA);
}

fn write_qname(data: *std.ArrayList(u8), qname: [][]u8) !void {
    var bw = std.io.bitWriter(.Big, data.writer());

    var cnt: usize = 0;
    while (cnt < qname.len) : (cnt += 1) {
        try bw.writeBits(@as(u8, @truncate(qname[cnt].len)), 8);
        _ = try bw.write(qname[cnt]);
    }

    try bw.writeBits(@as(u8, 0), 8);
    try bw.flushBits();
}

const DNSQuestion = struct {
    QNAME: [][]u8,
    QTYPE: enums.DNSQueryType,
    QCLASS: enums.DNSClassType,
};

const Answer = struct {
    QNAME: [][]u8,
    QTYPE: enums.DNSQueryType,
    QCLASS: enums.DNSClassType,
    TTL: u32,
    DATA_LENGTH: u16,
    DATA: []u8,

    pub fn toBytes(ans: Answer) ![]u8 {
        var b = std.ArrayList(u8).init(allocator);
        var bw = std.io.bitWriter(.Big, b.writer());

        var pn: usize = 0;
        while (pn < ans.QNAME.len) : (pn += 1) {
            var part = ans.QNAME[pn];
            bw.writeBits(@as(u8, @truncate(part.len)), 8);
            _ = try bw.write(part);
        }

        return b.items;
    }
};

const DNSResource = struct {
    DOMAIN_NAME: [][]u8,
    QTYPE: enums.DNSQueryType,
    QCLASS: enums.DNSClassType,
    TTL: u32,
    DATA_LENGTH: u16,
    DATA: []u8,
};

pub fn main() !void {
    try network.init();
    defer network.deinit();

    var sk = try network.Socket.create(.ipv4, .udp);
    defer sk.close();

    try sk.bindToPort(1053);
    print("listen on udp://{any}\n", .{sk.getLocalEndPoint()});

    while (true) {
        counter += 1;
        var buf = std.mem.zeroes([512]u8);
        var rf = try sk.receiveFrom(&buf);
        var packet = try parsePacket(&buf);
        var inf = try std.fmt.allocPrint(allocator, "{}.in.bin", .{counter});
        var infr = try std.fmt.allocPrint(allocator, "{}.in_recr.bin", .{counter});
        var outf = try std.fmt.allocPrint(allocator, "{}.out.bin", .{counter});
        var recr = try packet.toBytes();
        var resp = try generateResponse(packet);
        var rb = try resp.toBytes();
        try writeArray(&buf, rf.numberOfBytes, inf);
        try writeArray(recr, recr.len, infr);
        std.debug.print("orig:{}\n", .{std.fmt.fmtSliceHexUpper(buf[0..recr.len])});
        std.debug.print("recr:{}\n", .{std.fmt.fmtSliceHexUpper(recr[0..])});
        std.debug.print("repl:{any}\n", .{resp});
        _ = try sk.sendTo(rf.sender, rb);
        try writeArray(rb, rb.len, outf);
    }
}

fn generateResponse(packet: DNSPacket) !DNSPacket {
    var resp = packet;
    resp.QR = 1;
    resp.RA = packet.RD;
    resp.ANCOUNT = resp.QDCOUNT;
    resp.NSCOUNT = 0;
    resp.ARCOUNT = 0;
    var anidx: usize = 0;
    var answers = try allocator.alloc(Answer, resp.QDCOUNT);
    while (anidx < resp.QDCOUNT and resp.QDCOUNT != 0) : (anidx += 1) {
        answers[anidx] = Answer{
            .QNAME = packet.Questions[anidx].QNAME,
            .DATA_LENGTH = 4,
            .DATA = @constCast(&[4]u8{ 1, 1, 1, 1 }),
            .QTYPE = .A,
            .QCLASS = .IN,
            .TTL = 1,
        };
    }
    resp.Answers = answers;

    std.debug.print("{any}\n", .{resp});
    return resp;
}

fn parsePacket(data: []u8) !DNSPacket {
    var reader = std.io.fixedBufferStream(data[0..]); // Convert to a reader.
    var bit_reader = std.io.bitReader(std.builtin.Endian.Big, reader.reader());
    var bread: usize = undefined;
    var packet: DNSPacket = undefined;
    packet.ID = try bit_reader.readBits(u16, 16, &bread);
    packet.QR = try bit_reader.readBits(u1, 1, &bread);
    packet.OP = enums.parse_opcode(try bit_reader.readBits(u4, 4, &bread));
    packet.AA = try bit_reader.readBits(u1, 1, &bread);
    packet.TC = try bit_reader.readBits(u1, 1, &bread);
    packet.RD = try bit_reader.readBits(u1, 1, &bread);
    packet.RA = try bit_reader.readBits(u1, 1, &bread);
    packet.Z = try bit_reader.readBits(u3, 3, &bread);
    packet.RCODE = enums.parse_rcode(try bit_reader.readBits(u4, 4, &bread));
    packet.QDCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.ANCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.NSCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.ARCOUNT = try bit_reader.readBits(u16, 16, &bread);
    var questions = std.ArrayList(DNSQuestion).init(allocator);
    var i: usize = 0;
    while (i < packet.QDCOUNT) : (i += 1) {
        try questions.append(try readQuestion(&reader));
    }
    packet.Questions = questions.items;
    packet.Answers = try readAnswers(packet.ANCOUNT, &reader);
    std.log.info("QD:{}/AN:{}", .{ packet.QDCOUNT, packet.ANCOUNT });

    return packet;
}

fn readAnswers(count: u16, reader: *std.io.FixedBufferStream([]u8)) ![]Answer {
    var bit_reader = std.io.bitReader(std.builtin.Endian.Big, reader.reader());
    var resources = std.ArrayList(Answer).init(allocator);
    var i: u16 = 0;
    while (i < count and count != 0) : (i += 1) {
        var res: Answer = undefined;
        var parts = std.ArrayList([]u8).init(allocator);
        var stop = false;
        while (!stop) {
            var partlen = try bit_reader.readBitsNoEof(u8, 8);
            if (partlen == 0) {
                stop = true;
                break;
            }
            var part = try allocator.alloc(u8, partlen);
            _ = reader.read(part) catch {
                std.os.exit(0);
            };
            try parts.append(part);
        }
        if (stop) {
            continue;
        }
        res.QNAME = parts.items;
        var qtype = try bit_reader.readBitsNoEof(u16, 16);
        var qclass = try bit_reader.readBitsNoEof(u16, 16);
        res.QTYPE = enums.parse_qtype(qtype);
        res.QCLASS = enums.parse_classtype(qclass);
        res.TTL = try bit_reader.readBitsNoEof(u32, 32);
        res.DATA_LENGTH = try bit_reader.readBitsNoEof(u16, 16);
        var data = try allocator.alloc(u8, res.DATA_LENGTH);
        _ = try reader.read(data);
        res.DATA = data;
    }

    return resources.items;
}

fn readQuestion(reader: *std.io.FixedBufferStream([]u8)) !DNSQuestion {
    var question: DNSQuestion = undefined;
    var parts = std.ArrayList([]u8).init(allocator);
    var bit_reader = std.io.bitReader(std.builtin.Endian.Big, reader.reader());
    while (true) {
        var partlen = try bit_reader.readBitsNoEof(u8, 8);
        if (partlen == 0) {
            break;
        }
        var part = try allocator.alloc(u8, partlen);
        _ = reader.read(part) catch {
            std.os.exit(0);
        };
        try parts.append(part);
    }
    var qtype = try bit_reader.readBitsNoEof(u16, 16);
    var qclass = try bit_reader.readBitsNoEof(u16, 16);

    question.QNAME = parts.items;
    question.QTYPE = enums.parse_qtype(qtype);
    question.QCLASS = enums.parse_classtype(qclass);
    return question;
}

fn writeArray(buf: []u8, len: usize, filename: []u8) !void {
    if (!DUMP_PACKET) {
        return;
    }
    var buffer = try allocator.alloc(u8, len);
    var i: u32 = 0;
    while (i < len) : (i += 1) {
        buffer[i] = buf[i];
    }
    var path = std.fs.cwd();
    std.fs.cwd().makeDir("out") catch {};
    var outdir: std.fs.Dir = try path.openDir("out", .{});
    var file = try outdir.createFile(filename, .{});
    defer file.close();
    _ = try file.write(buffer);
}

test "simple test" {}
