const std = @import("std");
const usb = @import("zusb");
const clap = @import("clap");
const calibrate = @import("calibrate.zig");
const Calibration = @import("calibrate.zig").Calibration;
const vjoy = @import("vjoy.zig");
const Adapter = @import("adapter.zig").Adapter;
const Input = @import("adapter.zig").Input;
const Rumble = @import("adapter.zig").Rumble;
const Feeder = @import("feeder.zig").Feeder;
const ess = @import("ess/ess.zig");
const Atomic = std.atomic.Atomic;
const time = std.time;
const emulator = @import("emulator.zig");

pub const log_level = .info;

const Options = struct {
    ess_mapping: ?ess.Mapping,
    port: ?u16,
    use_calibration: bool,
    emulator_rumble: bool,
};

pub const Context = struct {
    allocator: std.mem.Allocator,
    mutex: std.Thread.Mutex,
    usb_ctx: *usb.Context,
    feeder: ?Feeder,
    receiver: *vjoy.FFBReceiver,
    stop: Atomic(bool),
    sock: ?*const std.x.os.Socket,
    ess_mapping: ?ess.Mapping,
    calibration: ?Calibration,
    use_calibration: bool,
    emulator_rumble: bool,
};

const fail_wait = 100 * time.ns_per_ms;

fn updateFeeder(context: *Context) !?Input {
    const feeder = &(context.feeder orelse unreachable);
    const adapter = &feeder.adapter;

    if (context.use_calibration) {
        if (context.calibration == null) {
            if (try Calibration.load(context.allocator)) |cal| {
                context.calibration = cal;
            } else {
                const cal = try calibrate.generateCalibration(adapter);
                try cal.save(context.allocator);
                context.calibration = cal;
            }
        }

        return feeder.feed(context.ess_mapping, context.calibration.?);
    } else {
        return feeder.feed(context.ess_mapping, null);
    }
}

fn inputLoop(context: *Context) void {
    while (!context.stop.load(.Acquire)) {
        if (context.feeder) |*feeder| {
            const input = updateFeeder(context) catch |err| {
                switch (err) {
                    error.Timeout => continue,
                    else => {
                        context.mutex.lock();
                        defer context.mutex.unlock();

                        feeder.deinit();
                        context.feeder = null;
                        std.log.err("{} in input thread", .{err});
                        std.log.info("Disconnected from adapter and vJoy", .{});
                        continue;
                    },
                }
            };

            if (context.sock) |s| {
                if (input) |in| {
                    var buffer: [@sizeOf(Input)]u8 = undefined;
                    in.serialize(&buffer);

                    _ = s.write(&buffer, 0) catch |err| {
                        std.log.err("{} in input thread", .{err});
                    };
                }
            }
        } else {
            context.mutex.lock();
            defer context.mutex.unlock();

            context.feeder = Feeder.init(context.usb_ctx) catch |err| {
                std.log.err("{} in input thread", .{err});
                time.sleep(fail_wait);
                continue;
            };

            std.log.info("Connected to adapter and vJoy", .{});
        }
    }
}

fn rumbleLoop(context: *Context) void {
    const receiver = context.receiver;
    var last_timestamp: ?i64 = null;
    var rumble = Rumble.Off;

    var handle: ?emulator.Handle = null;
    defer if (handle) |h| h.close();

    while (!context.stop.load(.Acquire)) {
        if (context.feeder) |*feeder| {
            if (!context.emulator_rumble) {
                if (receiver.get()) |packet| {
                    if (packet.device_id == 1) {
                        rumble = switch (packet.effect.operation) {
                            .Stop => .Off,
                            else => .On,
                        };

                        if (last_timestamp) |last| {
                            if (packet.timestamp_ms - last < 2) {
                                rumble = .Off;
                            }
                        }

                        last_timestamp = packet.timestamp_ms;
                    }
                }
            } else {
                if (handle) |h| {
                    rumble = h.rumbleState() catch blk: {
                        std.log.info("Disconnected from {s}", .{h.emulatorTitle()});
                        h.close();
                        handle = null;
                        break :blk .Off;
                    };
                } else {
                    handle = emulator.Handle.open() catch blk: {
                        time.sleep(fail_wait);
                        break :blk null;
                    };
                    if (handle) |h| std.log.info("Connected to {s} OoT 1.0", .{h.emulatorTitle()});
                    rumble = .Off;
                }
            }

            context.mutex.lock();

            feeder.adapter.setRumble(.{ rumble, .Off, .Off, .Off }) catch |err| {
                switch (err) {
                    error.Timeout => {
                        context.mutex.unlock();
                        continue;
                    },
                    else => {
                        // Release mutex before sleeping to allow input thread to acquire.
                        context.mutex.unlock();
                        std.log.err("{} in rumble thread", .{err});
                        time.sleep(fail_wait);
                        continue;
                    },
                }
            };

            context.mutex.unlock();
        } else {
            time.sleep(8 * time.ns_per_ms);
        }
    }
}

pub fn main() !void {
    const options = blk: {
        const params = comptime clap.parseParamsComptime(
            \\-h, --help          Display this help and exit.
            \\-e, --ess           Enables ESS adapter with oot-vc mapping.
            \\-m, --mapping <MAP> Enables ESS adapter with the specified mapping. Available mappings are: oot-vc, mm-vc, z64-gc.
            \\-s, --server        Enables UDP input server.
            \\-p, --port <PORT>   Enables UDP input server on port.
            \\-c, --calibrate     Use calibration to scale controller to full Windows range.
            \\--oot               Read rumble data from OoT 1.0 on emulator.
            \\
        );

        const parsers = comptime .{
            .MAP = clap.parsers.string,
            .PORT = clap.parsers.int(u16, 10),
        };

        var diag = clap.Diagnostic{};
        var res = clap.parse(clap.Help, &params, parsers, .{
            .diagnostic = &diag,
        }) catch |err| {
            diag.report(std.io.getStdErr().writer(), err) catch {};
            return;
        };
        defer res.deinit();

        if (res.args.help) {
            try clap.help(std.io.getStdErr().writer(), clap.Help, &params, .{});
            return;
        }

        const port = res.args.port orelse if (res.args.server)
            @as(u16, 4096)
        else
            null;

        const ess_mapping = if (res.args.mapping) |m|
            ess.Mapping.fromFileName(m) orelse {
                std.log.err("Invalid mapping specified.", .{});
                return;
            }
        else if (res.args.ess)
            ess.Mapping.oot_vc
        else
            null;

        break :blk Options{
            .ess_mapping = ess_mapping,
            .port = port,
            .use_calibration = res.args.calibrate,
            .emulator_rumble = res.args.oot,
        };
    };

    std.log.info("Initializing. Press enter to exit...", .{});

    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var ctx = try usb.Context.init();
    defer ctx.deinit();

    var receiver = try vjoy.FFBReceiver.init(allocator);
    defer receiver.deinit();

    const sock = blk: {
        if (options.port) |p| {
            const s = try std.x.os.Socket.init(
                std.os.AF.INET,
                std.os.SOCK.DGRAM,
                0,
                .{ .close_on_exec = true },
            );

            try s.connect(.{ .ipv4 = .{
                .host = std.x.os.IPv4.localhost,
                .port = p,
            } });

            std.log.info("Opened UDP server on port {}", .{p});
            break :blk s;
        } else {
            break :blk null;
        }
    };
    defer if (sock) |s| s.deinit();

    var thread_ctx = Context{
        .allocator = allocator,
        .mutex = std.Thread.Mutex{},
        .usb_ctx = &ctx,
        .feeder = null,
        .receiver = receiver,
        .stop = Atomic(bool).init(false),
        .sock = if (sock) |*s| s else null,
        .ess_mapping = options.ess_mapping,
        .calibration = null,
        .use_calibration = options.use_calibration,
        .emulator_rumble = options.emulator_rumble,
    };
    defer if (thread_ctx.feeder) |feeder| feeder.deinit();

    var threads = [_]std.Thread{
        try std.Thread.spawn(.{}, inputLoop, .{&thread_ctx}),
        try std.Thread.spawn(.{}, rumbleLoop, .{&thread_ctx}),
    };

    defer {
        thread_ctx.stop.store(true, .Release);

        for (threads) |thread| {
            thread.join();
        }
    }

    // Wait for user to press enter to exit program.
    _ = try std.io.getStdIn().reader().readByte();
}
