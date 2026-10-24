//! Simple Network Time Protocol (SNTPv4, RFC 4330) client messages.
//!
//! ```
//! use oxstack_net::sntp;
//! let request = sntp::request();
//! // send `request` over UDP to port 123, receive `reply`, then:
//! # let mut reply = [0u8; 48];
//! # reply[0] = 0b00_100_100; reply[1] = 2; reply[40..44].copy_from_slice(&3_970_000_000u32.to_be_bytes());
//! let t = sntp::parse_reply(&reply).unwrap();
//! assert!(t.unix_seconds > 1_700_000_000);
//! ```

/// UDP port SNTP servers listen on.
pub const PORT: u16 = 123;
/// Size of an SNTP message without extensions.
pub const PACKET_LEN: usize = 48;

/// Seconds from the NTP epoch (1900-01-01) to the Unix epoch (1970-01-01).
const NTP_TO_UNIX: u64 = 2_208_988_800;

/// Leap indicator 0, version 4, mode 3 (client).
const LI_VN_MODE_CLIENT: u8 = (4 << 3) | 3;
const MODE_SERVER: u8 = 4;
const MODE_BROADCAST: u8 = 5;

/// A client request: just the header byte, everything else zero (RFC 4330
/// §5 allows clients to leave the transmit timestamp zero).
pub const fn request() -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    p[0] = LI_VN_MODE_CLIENT;
    p
}

/// Time reported by a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ServerTime {
    /// Whole seconds since 1970-01-01 00:00:00 UTC.
    pub unix_seconds: u64,
    /// Sub-second part in microseconds.
    pub micros: u32,
    /// Server stratum (1 = directly attached to a reference clock).
    pub stratum: u8,
}

/// Why a reply was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Fewer than [`PACKET_LEN`] bytes.
    TooShort,
    /// Not a server (or broadcast) message.
    NotAServerReply,
    /// Stratum 0: a "Kiss-o'-Death" telling the client to back off. The
    /// four-character code (e.g. `RATE`, `DENY`) is included.
    KissOfDeath([u8; 4]),
    /// Leap indicator 3: the server's clock is not synchronised.
    Unsynchronised,
    /// Transmit timestamp is zero.
    NoTimestamp,
}

/// Validates a reply and extracts its transmit timestamp.
///
/// Network delay is not compensated: the result is the time the server
/// sent the reply, typically tens of milliseconds behind true time.
pub fn parse_reply(p: &[u8]) -> Result<ServerTime, Error> {
    let p: &[u8; PACKET_LEN] = p
        .get(..PACKET_LEN)
        .and_then(|s| s.try_into().ok())
        .ok_or(Error::TooShort)?;
    let mode = p[0] & 0x07;
    if mode != MODE_SERVER && mode != MODE_BROADCAST {
        return Err(Error::NotAServerReply);
    }
    let stratum = p[1];
    if stratum == 0 {
        return Err(Error::KissOfDeath([p[12], p[13], p[14], p[15]]));
    }
    if p[0] >> 6 == 3 {
        return Err(Error::Unsynchronised);
    }
    let secs = u32::from_be_bytes([p[40], p[41], p[42], p[43]]);
    let frac = u32::from_be_bytes([p[44], p[45], p[46], p[47]]);
    if secs == 0 && frac == 0 {
        return Err(Error::NoTimestamp);
    }
    // NTP era 0 ends in 2036; timestamps below 1970 in era 0 belong to era 1.
    let ntp_secs = if u64::from(secs) < NTP_TO_UNIX {
        u64::from(secs) + (1 << 32)
    } else {
        u64::from(secs)
    };
    Ok(ServerTime {
        unix_seconds: ntp_secs - NTP_TO_UNIX,
        micros: ((u64::from(frac) * 1_000_000) >> 32) as u32,
        stratum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(secs: u32, frac: u32) -> [u8; PACKET_LEN] {
        let mut p = [0u8; PACKET_LEN];
        p[0] = (4 << 3) | MODE_SERVER;
        p[1] = 2;
        p[40..44].copy_from_slice(&secs.to_be_bytes());
        p[44..48].copy_from_slice(&frac.to_be_bytes());
        p
    }

    #[test]
    fn request_header() {
        let r = request();
        assert_eq!(r[0], 0x23);
        assert!(r[1..].iter().all(|b| *b == 0));
    }

    #[test]
    fn converts_ntp_to_unix_in_both_eras() {
        // 2026-01-01T00:00:00Z = Unix 1_767_225_600 = NTP 3_976_214_400.
        let t = parse_reply(&reply(3_976_214_400, 1 << 31)).unwrap();
        assert_eq!(t.unix_seconds, 1_767_225_600);
        assert_eq!(t.micros, 500_000);
        assert_eq!(t.stratum, 2);

        // Era 1 (after the 2036 rollover): seconds = 10 → 2036-02-07T06:28:26Z.
        let t = parse_reply(&reply(10, 0)).unwrap();
        assert_eq!(t.unix_seconds, 2_085_978_506);
    }

    #[test]
    fn rejects_bad_replies() {
        assert_eq!(parse_reply(&[0; 10]), Err(Error::TooShort));

        let mut kod = reply(3_976_214_400, 0);
        kod[1] = 0;
        kod[12..16].copy_from_slice(b"RATE");
        assert_eq!(parse_reply(&kod), Err(Error::KissOfDeath(*b"RATE")));

        let mut client = reply(3_976_214_400, 0);
        client[0] = LI_VN_MODE_CLIENT;
        assert_eq!(parse_reply(&client), Err(Error::NotAServerReply));

        let mut unsync = reply(3_976_214_400, 0);
        unsync[0] |= 0xC0;
        assert_eq!(parse_reply(&unsync), Err(Error::Unsynchronised));

        assert_eq!(parse_reply(&reply(0, 0)), Err(Error::NoTimestamp));
    }
}
