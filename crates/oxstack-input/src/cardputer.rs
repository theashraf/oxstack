//! M5Stack Cardputer keyboard: 56 keys in 4 rows × 14 columns.

use crate::key::{Action, Key};

/// The physical layout, top-left to bottom-right.
#[rustfmt::skip]
pub const LAYOUT: [[Key; 14]; 4] = {
    use Key::*;
    [
        [Grave, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0, Minus, Equal, Backspace],
        [Tab, Q, W, E, R, T, Y, U, I, O, P, LeftBracket, RightBracket, Backslash],
        [Fn, Shift, A, S, D, F, G, H, J, K, L, Semicolon, Quote, Enter],
        [Ctrl, Opt, Alt, Z, X, C, V, B, N, M, Comma, Period, Slash, Space],
    ]
};

/// Scanner rows used on the Cardputer ADV's TCA8418 (`ROW0..ROW6`).
pub const ADV_MATRIX_ROWS: u8 = 7;
/// Scanner columns used on the Cardputer ADV's TCA8418 (`COL0..COL7`).
pub const ADV_MATRIX_COLS: u8 = 8;

/// Maps a TCA8418 matrix position on the **Cardputer ADV** to its key.
///
/// The ADV wires the 4×14 keyboard as a 7×8 matrix: each scanner row drives
/// two adjacent physical columns, and scanner columns 0–3 / 4–7 select the
/// physical row for the left / right one. (Wiring per the ADV schematic
/// and M5Stack's `M5Cardputer` library.)
pub const fn from_adv_matrix(row: u8, col: u8) -> Option<Key> {
    if row >= ADV_MATRIX_ROWS || col >= ADV_MATRIX_COLS {
        return None;
    }
    let phys_col = row * 2 + (col >= 4) as u8;
    let phys_row = col % 4;
    Some(LAYOUT[phys_row as usize][phys_col as usize])
}

/// The orange Fn-layer legends.
pub const fn fn_layer(key: Key) -> Option<Action> {
    use Key::*;
    Some(match key {
        Grave => Action::Escape,
        Num1 => Action::F(1),
        Num2 => Action::F(2),
        Num3 => Action::F(3),
        Num4 => Action::F(4),
        Num5 => Action::F(5),
        Num6 => Action::F(6),
        Num7 => Action::F(7),
        Num8 => Action::F(8),
        Num9 => Action::F(9),
        Num0 => Action::F(10),
        Minus => Action::F(11),
        Equal => Action::F(12),
        Backspace => Action::Delete,
        Semicolon => Action::Up,
        Comma => Action::Left,
        Period => Action::Down,
        Slash => Action::Right,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_lists_every_key_once_in_discriminant_order() {
        let flat = LAYOUT.as_flattened();
        assert_eq!(flat.len(), Key::COUNT);
        for (i, k) in flat.iter().enumerate() {
            assert_eq!(*k as usize, i);
        }
    }

    #[test]
    fn adv_matrix_maps_every_key_once() {
        let mut seen = 0u64;
        for row in 0..ADV_MATRIX_ROWS {
            for col in 0..ADV_MATRIX_COLS {
                let key = from_adv_matrix(row, col).unwrap();
                assert_eq!(seen & key.bit(), 0, "{key:?} mapped twice");
                seen |= key.bit();
            }
        }
        assert_eq!(seen.count_ones() as usize, Key::COUNT);
        assert_eq!(from_adv_matrix(7, 0), None);
        assert_eq!(from_adv_matrix(0, 8), None);

        // A bijection could still be the wrong permutation; pin the corners.
        assert_eq!(from_adv_matrix(0, 0), Some(Key::Grave));
        assert_eq!(from_adv_matrix(0, 4), Some(Key::Num1));
        assert_eq!(from_adv_matrix(0, 3), Some(Key::Ctrl));
        assert_eq!(from_adv_matrix(6, 7), Some(Key::Space));
        assert_eq!(from_adv_matrix(6, 6), Some(Key::Enter));
    }
}
