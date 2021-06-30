#version 330 core
#define BUTTON_A 0
#define BUTTON_B 1
#define BUTTON_X 2
#define BUTTON_Y 3
#define BUTTON_START 4
#define BUTTON_Z 5
#define STICK_MAIN 6
#define STICK_C 7
#define TRIGGER_LEFT 8
#define TRIGGER_RIGHT 9

#define SCREEN_POS (gl_FragCoord.xy / resolution)

uniform vec2 resolution;
uniform int which;

const vec4 main_color = vec4(0.95, 0.95, 0.95, 1.0);

vec4 colorButton(bool pressed) {
    switch (which) {
        case BUTTON_A:
            return vec4(0.0, 0.737, 0.556, 1.0);
        case BUTTON_B:
            return vec4(1.0, 0.0, 0.0, 1.0);
        case BUTTON_X:
        case BUTTON_Y:
        case BUTTON_START:
            return main_color;
        case BUTTON_Z:
            return vec4(0.333, 0.0, 0.678, 1.0);
        default:
            return vec4(1.0);
    }
}

vec4 colorStick(vec2 stick_pos) {
    switch (which) {
        case STICK_MAIN:
            return main_color;
        case STICK_C:
            return vec4(1.0, 0.894, 0.0, 1.0);
        default:
            return vec4(1.0);
    }
}

vec4 colorTrigger(float fill, bool pressed) {
    return main_color;
}