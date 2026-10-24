# oxstack-ir

Infrared remote-control encoders that produce mark/space timings in
microseconds, independent of any hardware. Feed the pulses to a pulse
generator (RMT on ESP32) modulated at the protocol's carrier frequency.

Supported: NEC and extended NEC.

```rust
use oxstack_ir::nec;

let frame = nec::encode(0x04, 0x08); // address, command
assert_eq!(frame.len(), nec::FRAME_PULSES);
assert_eq!((frame[0].mark_us, frame[0].space_us), (9000, 4500)); // leader
// transmit on a nec::CARRIER_HZ (38 kHz) carrier
```

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
