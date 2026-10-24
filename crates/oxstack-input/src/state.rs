use crate::cardputer;
use crate::key::{Action, Key, Modifiers};

/// A change in one key's state, with the modifiers held *after* the change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KeyEvent {
    /// The physical key.
    pub key: Key,
    /// `true` on press, `false` on release.
    pub pressed: bool,
    /// Modifiers held after this event was applied.
    pub mods: Modifiers,
}

impl KeyEvent {
    /// The action this event triggers: `None` for releases and for keys
    /// that only act as modifiers.
    ///
    /// Fn takes precedence over Shift, matching the Cardputer's printed
    /// orange legends. Ctrl, Alt and Opt don't change the resolved action;
    /// read them from [`mods`](Self::mods) for shortcuts.
    pub fn action(&self) -> Option<Action> {
        if !self.pressed {
            return None;
        }
        if self.mods.contains(Modifiers::FN)
            && let Some(action) = cardputer::fn_layer(self.key)
        {
            return Some(action);
        }
        match self.key {
            Key::Enter => Some(Action::Enter),
            Key::Backspace => Some(Action::Backspace),
            Key::Tab => Some(Action::Tab),
            key => {
                let (base, shifted) = key.legend()?;
                let shift = self.mods.contains(Modifiers::SHIFT);
                Some(Action::Char(if shift { shifted } else { base }))
            }
        }
    }
}

/// Held-key tracker. One `u64` bit per [`Key`], plus the derived modifiers.
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyboardState {
    held: u64,
    mods: Modifiers,
}

impl KeyboardState {
    /// Nothing held.
    pub const fn new() -> Self {
        Self {
            held: 0,
            mods: Modifiers::NONE,
        }
    }

    /// Records a press or release and returns the resulting event.
    pub fn apply(&mut self, key: Key, pressed: bool) -> KeyEvent {
        if pressed {
            self.held |= key.bit();
        } else {
            self.held &= !key.bit();
        }
        if key.modifier().is_some() {
            self.mods = self.derive_mods();
        }
        KeyEvent {
            key,
            pressed,
            mods: self.mods,
        }
    }

    /// Whether `key` is currently held.
    pub const fn is_held(&self, key: Key) -> bool {
        self.held & key.bit() != 0
    }

    /// Currently held modifiers.
    pub const fn modifiers(&self) -> Modifiers {
        self.mods
    }

    /// Forgets all held keys, e.g. after the scanner was reset.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    fn derive_mods(&self) -> Modifiers {
        [Key::Shift, Key::Ctrl, Key::Alt, Key::Opt, Key::Fn]
            .into_iter()
            .filter(|k| self.is_held(*k))
            .filter_map(Key::modifier)
            .fold(Modifiers::NONE, Modifiers::union)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(s: &mut KeyboardState, k: Key) -> KeyEvent {
        s.apply(k, true)
    }

    #[test]
    fn plain_and_shifted_characters() {
        let mut s = KeyboardState::new();
        assert_eq!(press(&mut s, Key::A).action(), Some(Action::Char('a')));
        press(&mut s, Key::Shift);
        assert_eq!(press(&mut s, Key::A).action(), Some(Action::Char('A')));
        assert_eq!(press(&mut s, Key::Num2).action(), Some(Action::Char('@')));
        s.apply(Key::Shift, false);
        assert_eq!(press(&mut s, Key::Num2).action(), Some(Action::Char('2')));
    }

    #[test]
    fn fn_layer_beats_shift() {
        let mut s = KeyboardState::new();
        press(&mut s, Key::Shift);
        press(&mut s, Key::Fn);
        assert_eq!(press(&mut s, Key::Semicolon).action(), Some(Action::Up));
        assert_eq!(press(&mut s, Key::Equal).action(), Some(Action::F(12)));
        // Keys without an orange legend fall through to the normal layer.
        assert_eq!(press(&mut s, Key::Q).action(), Some(Action::Char('Q')));
    }

    #[test]
    fn modifiers_and_releases() {
        let mut s = KeyboardState::new();
        let ev = press(&mut s, Key::Ctrl);
        assert_eq!(ev.action(), None, "modifier keys have no action");
        let ev = press(&mut s, Key::C);
        assert!(ev.mods.contains(Modifiers::CTRL));
        assert_eq!(ev.action(), Some(Action::Char('c')));
        assert_eq!(s.apply(Key::C, false).action(), None);
        s.apply(Key::Ctrl, false);
        assert!(s.modifiers().is_empty());
        assert!(!s.is_held(Key::Ctrl));
    }
}
