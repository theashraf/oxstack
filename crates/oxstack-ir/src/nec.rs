//! NEC protocol (the most common consumer IR format): 38 kHz carrier,
//! 9 ms + 4.5 ms leader, 32 data bits LSB-first, pulse-distance coded.

use crate::Pulse;

/// Carrier frequency in hertz.
pub const CARRIER_HZ: u32 = 38_000;
/// Pulses in one frame: leader, 32 bits, trailing mark.
pub const FRAME_PULSES: usize = 34;

const LEADER: Pulse = Pulse {
    mark_us: 9000,
    space_us: 4500,
};
const ZERO: Pulse = Pulse {
    mark_us: 562,
    space_us: 562,
};
const ONE: Pulse = Pulse {
    mark_us: 562,
    space_us: 1687,
};
const STOP: Pulse = Pulse {
    mark_us: 562,
    space_us: 0,
};
/// The "button held" repeat frame, sent every ~108 ms.
pub const REPEAT: [Pulse; 2] = [
    Pulse {
        mark_us: 9000,
        space_us: 2250,
    },
    STOP,
];

/// Encodes a standard NEC frame: address, !address, command, !command.
pub fn encode(address: u8, command: u8) -> [Pulse; FRAME_PULSES] {
    encode_raw(u32::from_le_bytes([address, !address, command, !command]))
}

/// Encodes an extended-NEC frame (16-bit address, no address inverse).
pub fn encode_extended(address: u16, command: u8) -> [Pulse; FRAME_PULSES] {
    let [lo, hi] = address.to_le_bytes();
    encode_raw(u32::from_le_bytes([lo, hi, command, !command]))
}

fn encode_raw(word: u32) -> [Pulse; FRAME_PULSES] {
    let mut out = [STOP; FRAME_PULSES];
    out[0] = LEADER;
    for bit in 0..32 {
        out[1 + bit] = if word >> bit & 1 == 1 { ONE } else { ZERO };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes bits back from pulse spaces, as a receiver would.
    fn decode(p: &[Pulse; FRAME_PULSES]) -> u32 {
        p[1..33]
            .iter()
            .enumerate()
            .fold(0, |w, (i, b)| w | (u32::from(b.space_us > 1000) << i))
    }

    #[test]
    fn standard_frame() {
        let f = encode(0x04, 0x08);
        let us = |p: &Pulse| (p.mark_us, p.space_us);
        assert_eq!(us(&f[0]), (9000, 4500), "leader");
        assert_eq!(us(&f[33]), (562, 0), "trailing mark");
        for (i, p) in f[1..33].iter().enumerate() {
            assert!(matches!(us(p), (562, 562 | 1687)), "bit {i}: {p:?}");
        }
        assert_eq!(decode(&f).to_le_bytes(), [0x04, 0xFB, 0x08, 0xF7]);
    }

    #[test]
    fn extended_frame_has_16_bit_address() {
        let w = decode(&encode_extended(0xBEEF, 0x12));
        assert_eq!(w.to_le_bytes(), [0xEF, 0xBE, 0x12, 0xED]);
    }
}
