# oxstack-audio

Hardware-independent, allocation-free audio building blocks for signed 16-bit
PCM: a level meter (peak, RMS, DC offset, dBFS) and a sine-tone generator
with a compile-time table and click-free frequency changes.

```rust
use oxstack_audio::{level::Level, tone::Sine};

// 10 ms of a 1 kHz tone at half scale, interleaved stereo.
let mut osc = Sine::new(1000.0, 48_000, 0.5);
let mut frames = [0i16; 2 * 480];
osc.fill(&mut frames, 2);

let left = Level::measure(&frames, 0, 2);
assert!((left.rms_dbfs() + 9.03).abs() < 0.1);
```

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
