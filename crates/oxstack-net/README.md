# oxstack-net

Network protocol pieces that don't depend on any socket implementation:
SNTP (RFC 4330) request and reply parsing, and Unix time to calendar time.
Move the bytes with whatever stack you have, such as `embassy-net`.

```rust
use oxstack_net::{sntp, time::DateTime};

let request = sntp::request(); // send over UDP to port sntp::PORT
# let mut reply = [0u8; 48];
# reply[0] = 0b00_100_100; reply[1] = 2;
# reply[40..44].copy_from_slice(&3_976_214_400u32.to_be_bytes());
let t = sntp::parse_reply(&reply).unwrap();
let now = DateTime::from_unix(t.unix_seconds as i64, 7 * 60); // UTC+7
assert_eq!((now.year, now.month, now.day, now.hour), (2026, 1, 1, 7));
```

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
