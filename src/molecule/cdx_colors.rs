//! Colours in a ChemDraw document: its colour table, and the colour indices that objects (an
//! atom's node, a bond) and text style runs name.
//!
//! A CDX document lists its colours once, in the document's ColorTable property (`0x0300`;
//! CDXML's `<colortable>`): a count, then each colour as three 16-bit channels. An object's
//! ForegroundColor (`0x0301`, CDXML's `color` attribute) and a style run's colour are an index:
//! 0 is black and 1 white, whatever the table says, and 2 on are the table's colours in order
//! (ChemDraw's own table starts with white and black, so its red is 4).
//!
//! Index 0 is the document's default foreground, and it is what a colourless atom, bond or text
//! is written and read as: [`None`] in the editor. Writing adds to the table the colours it does
//! not list yet, and only then rewrites it, so a document whose objects have no colour keeps its
//! table byte for byte (as [`FontTable`](super::cdx_fonts::FontTable) does for fonts). ChemDraw's
//! colours have no alpha: a translucent colour is written opaque, and read back opaque.

use super::cdx_codec::CdxObject;

/// kCDXProp_ColorTable.
pub(crate) const PROP_COLOR_TABLE: u16 = 0x0300;
/// kCDXProp_ForegroundColor: the colour index of an object.
pub(crate) const PROP_FOREGROUND_COLOR: u16 = 0x0301;
/// The index of the first colour of the table.
const FIRST_ENTRY: u16 = 2;

/// A document's colour table.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ColorTable {
    /// (red, green, blue) in 16 bits each, in the table's order.
    colors: Vec<[u16; 3]>,
    /// Whether colours were added since it was read.
    changed: bool,
}

/// A colour's 8-bit channel as ChemDraw's 16 bits (0xFF → 0xFFFF).
fn wide(v: u8) -> u16 {
    v as u16 * 257
}

/// A 16-bit channel back in 8 bits, rounded.
fn narrow(v: u16) -> u8 {
    ((v as u32 + 128) / 257) as u8
}

impl ColorTable {
    /// The colour table of `doc` (empty when it has none).
    pub fn of(doc: &CdxObject) -> Self {
        doc.prop(PROP_COLOR_TABLE).map(Self::parse).unwrap_or_default()
    }

    fn parse(v: &[u8]) -> Self {
        let u = |i: usize| v.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let mut table = ColorTable::default();
        for k in 0..u(0).unwrap_or(0) as usize {
            let at = 2 + k * 6;
            let (Some(r), Some(g), Some(b)) = (u(at), u(at + 2), u(at + 4)) else { break };
            table.colors.push([r, g, b]);
        }
        table
    }

    fn encode(&self) -> Vec<u8> {
        let mut v = (self.colors.len() as u16).to_le_bytes().to_vec();
        for c in &self.colors {
            for channel in c {
                v.extend_from_slice(&channel.to_le_bytes());
            }
        }
        v
    }

    /// The colour (straight RGBA, opaque) of colour index `index`: `None` for the default
    /// foreground (0) and for an index the table does not have.
    pub fn rgba(&self, index: u16) -> Option<[u8; 4]> {
        match index {
            0 => None,
            1 => Some([255, 255, 255, 255]),
            i => self.colors.get((i - FIRST_ENTRY) as usize).map(|&[r, g, b]| [narrow(r), narrow(g), narrow(b), 255]),
        }
    }

    /// The index of `color`: 0 (the default foreground) for `None`, else the first table entry
    /// of its red, green and blue, added to the table if it is not there yet.
    pub fn index_of(&mut self, color: Option<[u8; 4]>) -> u16 {
        let Some([r, g, b, _]) = color else { return 0 };
        let rgb = [wide(r), wide(g), wide(b)];
        if let Some(k) = self.colors.iter().position(|c| *c == rgb) {
            return FIRST_ENTRY + k as u16;
        }
        self.colors.push(rgb);
        self.changed = true;
        FIRST_ENTRY + (self.colors.len() - 1) as u16
    }

    /// Write the table back into `doc` if colours were added to it.
    pub fn store(&self, doc: &mut CdxObject) {
        if self.changed {
            doc.set(PROP_COLOR_TABLE, self.encode());
        }
    }
}

/// The colour `obj` names with its ForegroundColor, read through `table`.
pub(crate) fn foreground(obj: &CdxObject, table: &ColorTable) -> Option<[u8; 4]> {
    let v = obj.prop(PROP_FOREGROUND_COLOR).filter(|v| v.len() >= 2)?;
    table.rgba(u16::from_le_bytes([v[0], v[1]]))
}

/// Give `obj` the ForegroundColor of `color` (nothing for `None`), its index from `table`.
pub(crate) fn set_foreground(obj: &mut CdxObject, color: Option<[u8; 4]>, table: &mut ColorTable) {
    if color.is_some() {
        obj.set(PROP_FOREGROUND_COLOR, table.index_of(color).to_le_bytes().to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx;

    #[test]
    fn chemdraws_table_names_its_red_four_and_a_new_colour_comes_after_its_eight() {
        let mut doc = cdx::new_document();
        let mut table = ColorTable::of(&doc);
        assert_eq!(table.colors.len(), 8);
        assert_eq!(table.rgba(0), None, "the default foreground");
        assert_eq!(table.rgba(1), Some([255, 255, 255, 255]));
        assert_eq!(table.rgba(3), Some([0, 0, 0, 255]), "the table's black");
        assert_eq!(table.index_of(Some([255, 0, 0, 255])), 4, "ChemDraw's red");
        let before = doc.prop(PROP_COLOR_TABLE).unwrap().to_vec();
        table.store(&mut doc);
        assert_eq!(doc.prop(PROP_COLOR_TABLE).unwrap(), before.as_slice(), "nothing new: not rewritten");
        assert_eq!(table.index_of(Some([12, 34, 56, 128])), 10);
        assert_eq!(table.index_of(Some([12, 34, 56, 255])), 10, "one entry per colour, alpha aside");
        table.store(&mut doc);
        let read = ColorTable::of(&doc);
        assert_eq!(read.rgba(10), Some([12, 34, 56, 255]));
        assert_eq!(read.rgba(11), None, "past the table");
    }
}
