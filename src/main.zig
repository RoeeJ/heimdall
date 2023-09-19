const std = @import("std");
const network = @import("network");
const print = @import("std").debug.print;

var counter = @as(i32, 0);

const DNSPacket = packed struct {
    ID: u16, // ID
    QR: u1, // 0 for Query, 1 for Response
    OP: DNSOpcode,
    AA: u1, // Authoritative Answer
    TC: u1, // Truncated
    RD: u1, // Recursion Desired
    RA: u1, // Recursion Available
    Z: u3, // Unused
    RCODE: DNSResponseCode,
    QDCOUNT: u16,
    ANCOUNT: u16,
    NSCOUNT: u16,
    ARCOUNT: u16,
};

const DNSResponseCode = enum(u4) { no_error = 0, invalid_format = 1, server_error = 2, name_error = 3, request_not_supported = 4, policy_fail = 5, unk };

const DNSOpcode = enum(u4) { query = 0, iquery = 1, status = 2, notify = 4, update = 5, statefulupdate = 6, unk };

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
        var filename = try std.fmt.allocPrint(std.heap.page_allocator, "out/{}.bin", .{counter});
        try writeArray(&buf, rf.numberOfBytes, filename);
        std.log.info("Read {} bytes to socket, wrote to out/{}.bin", .{ rf.numberOfBytes, counter });
        const packet = try parsePacket(&buf);
        std.debug.print("{any}\n", .{packet});
    }
}

fn parse_opcode(b: u4) DNSOpcode {
    return switch (b) {
        0 => .query,
        1 => .iquery,
        2 => .status,
        4 => .notify,
        5 => .update,
        6 => .statefulupdate,
        else => .unk,
    };
}

fn parse_rcode(b: u4) DNSResponseCode {
    return switch (b) {
        0 => .no_error,
        1 => .invalid_format,
        2 => .server_error,
        3 => .name_error,
        4 => .request_not_supported,
        5 => .policy_fail,
        else => .unk,
    };
}

fn parsePacket(data: []u8) !DNSPacket {
    var reader = std.io.fixedBufferStream(data[0..]); // Convert to a reader.
    var bit_reader = std.io.bitReader(std.builtin.Endian.Big, reader.reader());
    var bread: usize = undefined;
    var packet: DNSPacket = undefined;
    packet.ID = try bit_reader.readBits(u16, 16, &bread);
    packet.QR = try bit_reader.readBits(u1, 1, &bread);
    packet.OP = parse_opcode(try bit_reader.readBits(u4, 4, &bread));
    packet.AA = try bit_reader.readBits(u1, 1, &bread);
    packet.TC = try bit_reader.readBits(u1, 1, &bread);
    packet.RD = try bit_reader.readBits(u1, 1, &bread);
    packet.RA = try bit_reader.readBits(u1, 1, &bread);
    packet.Z = try bit_reader.readBits(u3, 3, &bread);
    packet.RCODE = parse_rcode(try bit_reader.readBits(u4, 4, &bread));
    packet.QDCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.ANCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.NSCOUNT = try bit_reader.readBits(u16, 16, &bread);
    packet.ARCOUNT = try bit_reader.readBits(u16, 16, &bread);

    return packet;
}
fn writeArray(buf: []u8, len: usize, filename: []u8) !void {
    var buffer = try std.heap.page_allocator.alloc(u8, len);
    var i: u32 = 0;
    while (i < len) : (i += 1) {
        buffer[i] = buf[i];
    }
    var file = try std.fs.cwd().createFile(filename, .{});
    defer file.close();
    _ = try file.write(buffer);
}
test "simple test" {}
