/// A physical key, named after its primary legend.
///
/// The discriminants are dense (`0..Key::COUNT`) so a set of held keys fits
/// in one `u64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
#[allow(missing_docs)]
pub enum Key {
    // Row 0
    Grave,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Num0,
    Minus,
    Equal,
    Backspace,
    // Row 1
    Tab,
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    LeftBracket,
    RightBracket,
    Backslash,
    // Row 2
    Fn,
    Shift,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    Semicolon,
    Quote,
    Enter,
    // Row 3
    Ctrl,
    Opt,
    Alt,
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
    Comma,
    Period,
    Slash,
    Space,
}

impl Key {
    /// Number of keys.
    pub const COUNT: usize = Key::Space as usize + 1;

    /// Unshifted and shifted characters printed on the key, for keys that
    /// type a character.
    pub const fn legend(self) -> Option<(char, char)> {
        use Key::*;
        Some(match self {
            Grave => ('`', '~'),
            Num1 => ('1', '!'),
            Num2 => ('2', '@'),
            Num3 => ('3', '#'),
            Num4 => ('4', '$'),
            Num5 => ('5', '%'),
            Num6 => ('6', '^'),
            Num7 => ('7', '&'),
            Num8 => ('8', '*'),
            Num9 => ('9', '('),
            Num0 => ('0', ')'),
            Minus => ('-', '_'),
            Equal => ('=', '+'),
            Q => ('q', 'Q'),
            W => ('w', 'W'),
            E => ('e', 'E'),
            R => ('r', 'R'),
            T => ('t', 'T'),
            Y => ('y', 'Y'),
            U => ('u', 'U'),
            I => ('i', 'I'),
            O => ('o', 'O'),
            P => ('p', 'P'),
            LeftBracket => ('[', '{'),
            RightBracket => (']', '}'),
            Backslash => ('\\', '|'),
            A => ('a', 'A'),
            S => ('s', 'S'),
            D => ('d', 'D'),
            F => ('f', 'F'),
            G => ('g', 'G'),
            H => ('h', 'H'),
            J => ('j', 'J'),
            K => ('k', 'K'),
            L => ('l', 'L'),
            Semicolon => (';', ':'),
            Quote => ('\'', '"'),
            Z => ('z', 'Z'),
            X => ('x', 'X'),
            C => ('c', 'C'),
            V => ('v', 'V'),
            B => ('b', 'B'),
            N => ('n', 'N'),
            M => ('m', 'M'),
            Comma => (',', '<'),
            Period => ('.', '>'),
            Slash => ('/', '?'),
            Space => (' ', ' '),
            Backspace | Tab | Fn | Shift | Enter | Ctrl | Opt | Alt => return None,
        })
    }

    /// The modifier this key holds down, if it is a modifier key.
    pub const fn modifier(self) -> Option<Modifiers> {
        match self {
            Key::Shift => Some(Modifiers::SHIFT),
            Key::Ctrl => Some(Modifiers::CTRL),
            Key::Alt => Some(Modifiers::ALT),
            Key::Opt => Some(Modifiers::OPT),
            Key::Fn => Some(Modifiers::FN),
            _ => None,
        }
    }

    pub(crate) const fn bit(self) -> u64 {
        1 << self as u8
    }
}

/// Held modifier keys, as a bit set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(1 << 0);
    /// Ctrl.
    pub const CTRL: Self = Self(1 << 1);
    /// Alt.
    pub const ALT: Self = Self(1 << 2);
    /// Opt (the Cardputer's fourth modifier).
    pub const OPT: Self = Self(1 << 3);
    /// Fn (selects the orange legends).
    pub const FN: Self = Self(1 << 4);

    /// Whether every modifier in `other` is held.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Union of two sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether no modifier is held.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// What a key press means once legends and layers are applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Action {
    /// A printable character.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Delete the character before the cursor.
    Backspace,
    /// Delete the character after the cursor (Fn + Backspace).
    Delete,
    /// Tab.
    Tab,
    /// Escape (Fn + `).
    Escape,
    /// Cursor up (Fn + ;).
    Up,
    /// Cursor down (Fn + .).
    Down,
    /// Cursor left (Fn + ,).
    Left,
    /// Cursor right (Fn + /).
    Right,
    /// Function key `F1..=F12` (Fn + 1 … Fn + =).
    F(u8),
}
