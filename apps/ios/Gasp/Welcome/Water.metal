#include <metal_stdlib>
#include <SwiftUI/SwiftUI_Metal.h>
using namespace metal;

// The website's moving water (feTurbulence into feDisplacementMap) as
// SwiftUI shaders: smooth value noise drifting with time bends whatever
// lies beneath it, and rings set off by a finger spread across it.

namespace water {

constant float ringSpeed = 300.0;
constant float ringWavelength = 56.0;
constant float ringPacket = 70.0;
constant float ringFade = 0.8;
constant float ringLife = 2.6;
constant float ringLift = 9.0;

float hash(float2 cell) {
    float2 scrambled = fract(cell * float2(123.34, 456.21));
    scrambled += dot(scrambled, scrambled + 45.32);
    return fract(scrambled.x * scrambled.y);
}

float valueNoise(float2 point) {
    float2 cell = floor(point);
    float2 inside = fract(point);
    float2 ease = inside * inside * (3.0 - 2.0 * inside);
    float bottom = mix(hash(cell), hash(cell + float2(1, 0)), ease.x);
    float top = mix(hash(cell + float2(0, 1)), hash(cell + float2(1, 1)), ease.x);
    return mix(bottom, top, ease.y);
}

float fractalNoise(float2 point) {
    return valueNoise(point) * 0.65 + valueNoise(point * 2.03 + 17.1) * 0.35;
}

// How far the water bends the point, each axis from -1 to 1.
float2 drift(float2 position, float time, float2 frequency) {
    float2 scaled = position * frequency;
    float across = fractalNoise(scaled + float2(time * 0.13, time * 0.31)) - 0.5;
    float down = fractalNoise(scaled * 1.27 + float2(7.3 - time * 0.19, 2.1 + time * 0.23)) - 0.5;
    return float2(across, down) * 2.0;
}

// One ring: x and y of where the finger touched, its age in seconds and
// its strength, 0 for no ring.
float2 ring(float2 position, float4 ripple) {
    float age = ripple.z;
    if (ripple.w <= 0.0 || age < 0.0 || age > ringLife) return float2(0);
    float2 away = position - ripple.xy;
    float distance = max(length(away), 0.001);
    float behindFront = distance - ringSpeed * age;
    float envelope = exp(-pow(behindFront / ringPacket, 2.0)) * exp(-age / ringFade);
    float lift = ringLift * ripple.w * envelope * sin(2.0 * M_PI_F * behindFront / ringWavelength);
    return away / distance * lift;
}

}

[[stitchable]] float2 waterDistortion(
    float2 position, float time, float strength, float2 frequency,
    float4 ring0, float4 ring1, float4 ring2, float4 ring3
) {
    float2 bend = water::drift(position, time, frequency) * strength;
    bend += water::ring(position, ring0) + water::ring(position, ring1);
    bend += water::ring(position, ring2) + water::ring(position, ring3);
    return position + bend;
}

constant float2 softTaps[8] = {
    float2(1, 0), float2(0.7071, 0.7071), float2(0, 1), float2(-0.7071, 0.7071),
    float2(-1, 0), float2(-0.7071, -0.7071), float2(0, -1), float2(0.7071, -0.7071)
};

// A glyph seen through slow water and rounded like a drop of ink: the
// bent glyph is blurred a little, then its edge pulled back to crisp.
[[stitchable]] half4 softGlyph(float2 position, SwiftUI::Layer layer, float time, float strength, float softness) {
    float2 source = position + water::drift(position, time, float2(0.006, 0.02)) * strength;
    half4 sum = layer.sample(source) * 2.0h;
    for (int tap = 0; tap < 8; tap++) {
        sum += layer.sample(source + softTaps[tap] * softness * 0.5) * 1.5h;
        sum += layer.sample(source + softTaps[tap] * softness);
    }
    sum /= 22.0h;
    if (sum.a < 0.002h) return half4(0);
    half3 color = sum.rgb / sum.a;
    half alpha = smoothstep(0.28h, 0.62h, sum.a);
    return half4(color * alpha, alpha);
}
