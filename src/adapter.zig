// TODO: Remove this file and instead create a generic controller api. This is a leftover from gcfeeder.
const std = @import("std");

const Vec2 = struct {
    x: u8,
    y: u8,
};

const StickRange = struct {
    center: u8,
    radius: u8,

    pub fn restrict(self: StickRange, x: i10, y: i10) Vec2 {
        const center = @as(i10, self.center);
        const radius = @as(i10, self.radius);

        const xx = std.math.clamp(x, center - radius, center + radius);
        const yy = std.math.clamp(y, center - radius, center + radius);

        return Vec2{
            .x = @intCast(u8, xx),
            .y = @intCast(u8, yy),
        };
    }

    pub fn normalize(self: StickRange, axis: u8) f64 {
        return @intToFloat(f64, axis) / @intToFloat(f64, self.center + self.radius);
    }
};

const AnalogRange = struct {
    min: u8,
    max: u8,

    pub fn restrict(self: AnalogRange, n: i10) u8 {
        const nn = std.math.clamp(n, @as(i10, self.min), @as(i10, self.max));

        return @intCast(u8, nn);
    }

    pub fn normalize(self: AnalogRange, axis: u8) f64 {
        return @intToFloat(f64, axis + self.min) / @intToFloat(f64, self.max - self.min);
    }
};

pub const Calibration = struct {
    stick_x: i10,
    stick_y: i10,
    substick_x: i10,
    substick_y: i10,
    trigger_left: i10,
    trigger_right: i10,

    pub const stick_range = StickRange{ .center = 0x80, .radius = 0x7F };
    pub const trigger_range = AnalogRange{ .min = 0x00, .max = 0xFF };

    pub fn init(initial: Input) Calibration {
        return Calibration{
            .stick_x = @as(i10, stick_range.center) - initial.stick_x,
            .stick_y = @as(i10, stick_range.center) - initial.stick_y,
            .substick_x = @as(i10, stick_range.center) - initial.substick_x,
            .substick_y = @as(i10, stick_range.center) - initial.substick_y,
            .trigger_left = @as(i10, trigger_range.min) - initial.trigger_left,
            .trigger_right = @as(i10, trigger_range.min) - initial.trigger_right,
        };
    }

    pub fn correct(self: Calibration, input: Input) Input {
        var in = input;

        const stick = stick_range.restrict(
            @as(i10, in.stick_x) + self.stick_x,
            @as(i10, in.stick_y) + self.stick_y,
        );

        const substick = stick_range.restrict(
            @as(i10, in.substick_x) + self.substick_x,
            @as(i10, in.substick_y) + self.substick_y,
        );

        in.stick_x = stick.x;
        in.stick_y = stick.y;
        in.substick_x = substick.x;
        in.substick_y = substick.y;
        in.trigger_left = trigger_range.restrict(@as(i10, in.trigger_left) + self.trigger_left);
        in.trigger_right = trigger_range.restrict(@as(i10, in.trigger_right) + self.trigger_right);

        return in;
    }
};

pub const Input = struct {
    button_a: bool,
    button_b: bool,
    button_x: bool,
    button_y: bool,

    button_left: bool,
    button_right: bool,
    button_down: bool,
    button_up: bool,

    button_start: bool,
    button_z: bool,
    button_r: bool,
    button_l: bool,

    stick_x: u8,
    stick_y: u8,
    substick_x: u8,
    substick_y: u8,
    trigger_left: u8,
    trigger_right: u8,

    pub fn deserialize(buffer: *const [@sizeOf(Input)]u8) Input {
        var input: Input = undefined;

        inline for (@typeInfo(Input).Struct.fields) |field, i| {
            @field(input, field.name) = switch (field.type) {
                u8 => buffer[i],
                bool => buffer[i] != 0,
                else => @compileError("Unsupported type"),
            };
        }

        return input;
    }
};
