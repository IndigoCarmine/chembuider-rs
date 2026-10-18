//! The CDX container: a tree of tagged objects holding tagged properties, as ChemDraw writes it.
//! This layer knows nothing of chemistry — [`cdx`](super::cdx) gives the tags their meaning.
//!
//! Layout: the 22-byte header (`VjCD0100`, `04 03 02 01`, ten zero bytes), then the document
//! object. An object is its tag (high bit set), a u32 id, its properties and child objects, and a
//! zero tag. A property is its tag, a u16 length (`0xFFFF` then a u32 when longer), and the bytes.
//! Everything is little-endian.

/// The bytes every CDX file begins with.
pub const HEADER: [u8; 22] = [
    b'V', b'j', b'C', b'D', b'0', b'1', b'0', b'0', 0x04, 0x03, 0x02, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0,
];

/// One object: its tag, id, properties in file order, and the objects inside it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CdxObject {
    pub tag: u16,
    pub id: u32,
    pub props: Vec<(u16, Vec<u8>)>,
    pub children: Vec<CdxObject>,
}

impl CdxObject {
    pub fn new(tag: u16, id: u32) -> Self {
        Self { tag, id, ..Default::default() }
    }

    /// The value of property `tag`, if present.
    pub fn prop(&self, tag: u16) -> Option<&[u8]> {
        self.props.iter().find(|(t, _)| *t == tag).map(|(_, v)| v.as_slice())
    }

    /// Set property `tag` (replacing an earlier value, keeping its place).
    pub fn set(&mut self, tag: u16, value: Vec<u8>) {
        match self.props.iter_mut().find(|(t, _)| *t == tag) {
            Some(p) => p.1 = value,
            None => self.props.push((tag, value)),
        }
    }

    pub fn prop_i16(&self, tag: u16) -> Option<i16> {
        let v = self.prop(tag)?;
        Some(match v.len() {
            1 => v[0] as i8 as i16,
            n if n >= 2 => i16::from_le_bytes([v[0], v[1]]),
            _ => return None,
        })
    }

    /// An integer property of any width (1, 2 or 4 bytes), sign-extended.
    pub fn prop_int(&self, tag: u16) -> Option<i32> {
        let v = self.prop(tag)?;
        Some(match v.len() {
            1 => v[0] as i8 as i32,
            2 | 3 => i16::from_le_bytes([v[0], v[1]]) as i32,
            n if n >= 4 => i32::from_le_bytes([v[0], v[1], v[2], v[3]]),
            _ => return None,
        })
    }

    pub fn prop_u32(&self, tag: u16) -> Option<u32> {
        let v = self.prop(tag)?;
        (v.len() >= 4).then(|| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }

    /// The largest id in this object and everything inside it.
    pub fn max_id(&self) -> u32 {
        self.children.iter().map(CdxObject::max_id).fold(self.id, u32::max)
    }
}

/// The document object of a CDX file, or `None` when the bytes are not one. Bytes after the
/// document (ChemDraw's OLE `CONTENTS` carries two) are ignored.
pub fn parse(bytes: &[u8]) -> Option<CdxObject> {
    if bytes.len() < HEADER.len() || bytes[..8] != HEADER[..8] {
        return None;
    }
    let mut at = HEADER.len();
    let tag = read_u16(bytes, &mut at)?;
    if tag & 0x8000 == 0 {
        return None;
    }
    let id = read_u32(bytes, &mut at)?;
    let mut doc = CdxObject::new(tag, id);
    parse_body(bytes, &mut at, &mut doc, 0)?;
    Some(doc)
}

/// Objects are not nested deeper than this in anything ChemDraw writes; a guard against
/// a malformed file running the parser out of stack.
const MAX_DEPTH: usize = 64;

fn parse_body(bytes: &[u8], at: &mut usize, obj: &mut CdxObject, depth: usize) -> Option<()> {
    if depth > MAX_DEPTH {
        return None;
    }
    loop {
        let tag = read_u16(bytes, at)?;
        if tag == 0 {
            return Some(());
        }
        if tag & 0x8000 != 0 {
            let id = read_u32(bytes, at)?;
            let mut child = CdxObject::new(tag, id);
            parse_body(bytes, at, &mut child, depth + 1)?;
            obj.children.push(child);
        } else {
            let mut len = read_u16(bytes, at)? as usize;
            if len == 0xFFFF {
                len = read_u32(bytes, at)? as usize;
            }
            let value = bytes.get(*at..at.checked_add(len)?)?.to_vec();
            *at += len;
            obj.props.push((tag, value));
        }
    }
}

fn read_u16(bytes: &[u8], at: &mut usize) -> Option<u16> {
    let v = bytes.get(*at..*at + 2)?;
    *at += 2;
    Some(u16::from_le_bytes([v[0], v[1]]))
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let v = bytes.get(*at..*at + 4)?;
    *at += 4;
    Some(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}

/// The CDX file for `doc`, ending with a zero tag after the document as ChemDraw's own do.
pub fn write(doc: &CdxObject) -> Vec<u8> {
    let mut out = HEADER.to_vec();
    write_object(&mut out, doc);
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn write_object(out: &mut Vec<u8>, obj: &CdxObject) {
    out.extend_from_slice(&obj.tag.to_le_bytes());
    out.extend_from_slice(&obj.id.to_le_bytes());
    for (tag, value) in &obj.props {
        out.extend_from_slice(&tag.to_le_bytes());
        if value.len() < 0xFFFF {
            out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        } else {
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        }
        out.extend_from_slice(value);
    }
    for child in &obj.children {
        write_object(out, child);
    }
    out.extend_from_slice(&0u16.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ChemDraw's own bytes come back unchanged: the codec loses nothing it does not understand.
    #[test]
    fn real_files_round_trip_byte_for_byte() {
        for bytes in [
            &include_bytes!("../../tests/fixtures/chemdraw_real.cdx")[..],
            &include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx")[..],
        ] {
            let doc = parse(bytes).expect("parses");
            assert_eq!(doc.tag, 0x8000);
            let written = write(&doc);
            assert_eq!(&written[..], &bytes[..written.len()], "same bytes");
        }
    }

    #[test]
    fn long_properties_use_the_extended_length() {
        let mut doc = CdxObject::new(0x8000, 0);
        doc.props.push((0x0003, vec![7; 70_000]));
        let back = parse(&write(&doc)).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn rejects_garbage_and_truncation() {
        assert!(parse(b"not a cdx").is_none());
        let bytes = include_bytes!("../../tests/fixtures/chemdraw_real.cdx");
        assert!(parse(&bytes[..bytes.len() / 2]).is_none());
    }
}
