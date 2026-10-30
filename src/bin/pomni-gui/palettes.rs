//! Local color favorites and confirmed-use history; never stores device settings.
use super::{SWATCHES, parse_hex};

pub const STORAGE_KEY: &str = "palette_book_v1";

#[derive(Debug, PartialEq, Eq)]
pub struct PaletteBook {
    pub favorites: [[u8; 3]; 8],
    pub recent: Vec<[u8; 3]>,
}

impl Default for PaletteBook {
    fn default() -> Self {
        Self {
            favorites: SWATCHES,
            recent: Vec::new(),
        }
    }
}

impl PaletteBook {
    pub fn reset_favorites(&mut self) {
        self.favorites = SWATCHES;
    }

    /// Most recently confirmed color first, unique, with a maximum of eight.
    pub fn remember(&mut self, color: [u8; 3]) {
        self.recent.retain(|old| *old != color);
        self.recent.insert(0, color);
        self.recent.truncate(8);
    }

    pub fn encode(&self) -> String {
        let colors = |items: &[[u8; 3]]| {
            items
                .iter()
                .map(|[r, g, b]| format!("{r:02X}{g:02X}{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        format!("v1\n{}\n{}", colors(&self.favorites), colors(&self.recent))
    }

    /// Missing/corrupt entries fall back individually; unknown versions are ignored.
    pub fn decode(value: &str) -> Self {
        let mut book = Self::default();
        let mut lines = value.lines();
        if lines.next() != Some("v1") {
            return book;
        }
        if let Some(line) = lines.next() {
            for (slot, value) in book.favorites.iter_mut().zip(line.split_whitespace()) {
                if let Some(color) = parse_hex(value) {
                    *slot = color;
                }
            }
        }
        if let Some(line) = lines.next() {
            for color in line.split_whitespace().filter_map(parse_hex) {
                if !book.recent.contains(&color) {
                    book.recent.push(color);
                }
                if book.recent.len() == 8 {
                    break;
                }
            }
        }
        book
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_restores_default_favorites_and_keeps_history() {
        let mut book = PaletteBook {
            favorites: [[1, 2, 3]; 8],
            ..Default::default()
        };
        book.remember([10, 20, 30]);
        book.reset_favorites();
        assert_eq!(book.favorites, SWATCHES);
        assert_eq!(book.recent, vec![[10, 20, 30]]);
        assert_eq!(PaletteBook::decode(&book.encode()), book);
    }

    #[test]
    fn ninth_color_evicts_oldest_and_reuse_moves_to_front() {
        let mut book = PaletteBook::default();
        for i in 0..9 {
            book.remember([i, 10, 20]);
        }
        assert_eq!(
            book.recent,
            (1..9).rev().map(|i| [i, 10, 20]).collect::<Vec<_>>()
        );
        book.remember([3, 10, 20]);
        assert_eq!(book.recent, [3, 8, 7, 6, 5, 4, 2, 1].map(|i| [i, 10, 20]));
        assert_eq!(book.favorites, SWATCHES);
    }

    #[test]
    fn edited_favorites_and_history_survive_restart() {
        let mut book = PaletteBook::default();
        book.favorites[0] = [1, 2, 3];
        book.favorites[7] = [4, 5, 6];
        book.remember([40, 50, 60]);
        book.remember([70, 80, 90]);
        assert_eq!(PaletteBook::decode(&book.encode()), book);
    }

    #[test]
    fn corrupt_saved_data_keeps_valid_slots_and_bounds_history() {
        let book = PaletteBook::decode(
            "v1\n010203 broken 040506\n000000 000000 invalid 010101 020202 030303 040404 050505 060606 070707 080808",
        );
        assert_eq!(book.favorites[0], [1, 2, 3]);
        assert_eq!(book.favorites[1], SWATCHES[1]);
        assert_eq!(book.favorites[2], [4, 5, 6]);
        assert_eq!(book.favorites[7], SWATCHES[7]);
        assert_eq!(book.recent, (0..8).map(|i| [i; 3]).collect::<Vec<_>>());
        assert_eq!(PaletteBook::decode("v2\n010203"), PaletteBook::default());
        assert_eq!(PaletteBook::decode(""), PaletteBook::default());
    }
}
