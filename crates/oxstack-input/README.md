# oxstack-input

A `no_std` keyboard model: physical keys, held modifiers, the Fn layer, and
the M5Stack Cardputer's matrix maps. It turns raw scanner positions into
actions such as characters and arrow keys, with no allocation.

```rust
use oxstack_input::{Action, Key, KeyboardState};

let mut keys = KeyboardState::new();
keys.apply(Key::Shift, true);
assert_eq!(keys.apply(Key::A, true).action(), Some(Action::Char('A')));

keys.apply(Key::Fn, true);
assert_eq!(keys.apply(Key::Semicolon, true).action(), Some(Action::Up));
```

Map a Cardputer ADV scanner position to a key with
`cardputer::from_adv_matrix(row, col)`.

Part of [oxstack](https://github.com/theashraf/oxstack). Licensed under the MIT license.
