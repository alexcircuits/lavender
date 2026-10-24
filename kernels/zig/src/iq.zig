//! High-Performance I/Q Demodulator for Mode-S ADS-B (2 Msps)
//!
//! Processes unsigned 8-bit interleaved I/Q samples from RTL-SDR dongles,
//! computes signal power, detects preamble pulse sequences, and extracts
//! Manchester-encoded 112-bit frames.

const std = @import("std");
const adsb = @import("adsb.zig");

pub const PREAMBLE_SAMPLES: usize = 16;
pub const FRAME_BITS: usize = 112;
pub const FRAME_SAMPLES: usize = PREAMBLE_SAMPLES + FRAME_BITS * 2;

/// Checks preamble pulses at offsets [0, 2, 7, 9] vs quiet intervals [1, 3, 4, 5, 6, 8].
/// Returns non-zero contrast if a plausible Mode S preamble is found.
pub fn preambleContrast(power: []const f32, offset: usize) f32 {
    if (offset + PREAMBLE_SAMPLES > power.len) return 0.0;

    const p0 = power[offset + 0];
    const p2 = power[offset + 2];
    const p7 = power[offset + 7];
    const p9 = power[offset + 9];
    const pulse_mean = (p0 + p2 + p7 + p9) * 0.25;

    const q1 = power[offset + 1];
    const q3 = power[offset + 3];
    const q4 = power[offset + 4];
    const q5 = power[offset + 5];
    const q6 = power[offset + 6];
    const q8 = power[offset + 8];
    const quiet_mean = (q1 + q3 + q4 + q5 + q6 + q8) / 6.0;

    const contrast = pulse_mean - quiet_mean;
    if (contrast < 120.0 or pulse_mean < quiet_mean * 1.8) return 0.0;

    const decision_level = quiet_mean + contrast * 0.35;
    if (p0 <= decision_level or p2 <= decision_level or p7 <= decision_level or p9 <= decision_level) {
        return 0.0;
    }

    return contrast;
}

/// Demodulates raw unsigned 8-bit interleaved I/Q buffer into extracted 14-byte frames.
/// Returns number of valid frames written into `out_frames`.
pub fn extractAdsbFrames(
    iq: []const u8,
    power_scratch: []f32,
    out_frames: [][14]u8,
) usize {
    const sample_count = iq.len / 2;
    if (sample_count < FRAME_SAMPLES) return 0;
    if (power_scratch.len < sample_count) return 0;

    // Convert u8 I/Q to centered power: (i - 127.5)^2 + (q - 127.5)^2
    var i: usize = 0;
    while (i < sample_count) : (i += 1) {
        const i_sample = @as(f32, @floatFromInt(iq[i * 2])) - 127.5;
        const q_sample = @as(f32, @floatFromInt(iq[i * 2 + 1])) - 127.5;
        power_scratch[i] = i_sample * i_sample + q_sample * q_sample;
    }

    var frame_count: usize = 0;
    var offset: usize = 0;
    const max_offset = sample_count - FRAME_SAMPLES;

    while (offset <= max_offset and frame_count < out_frames.len) : (offset += 1) {
        const contrast = preambleContrast(power_scratch, offset);
        if (contrast <= 0.0) continue;

        var bytes: [14]u8 = [_]u8{0} ** 14;
        var uncertain_bits: usize = 0;

        for (0..FRAME_BITS) |bit_idx| {
            const s_offset = offset + PREAMBLE_SAMPLES + bit_idx * 2;
            const first = power_scratch[s_offset];
            const second = power_scratch[s_offset + 1];
            const diff = @abs(first - second);
            const total = first + second;

            const min_diff = if (20.0 > contrast * 0.02) 20.0 else contrast * 0.02;
            const denom = if (total > 1.0) total else 1.0;

            if (diff < min_diff or (diff / denom) < 0.04) {
                uncertain_bits += 1;
            }

            if (first > second) {
                bytes[bit_idx >> 3] |= @as(u8, 1) << @intCast(7 - (bit_idx & 7));
            }
        }

        if (uncertain_bits <= 10 and adsb.modeSChecksum(&bytes) == 0) {
            out_frames[frame_count] = bytes;
            frame_count += 1;
            offset += FRAME_SAMPLES - 1; // Skip ahead past decoded frame
        }
    }

    return frame_count;
}
