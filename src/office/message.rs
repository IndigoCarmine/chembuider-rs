//! The messages of ibisOffice's external-editor protocol (version 1), as JSON objects tagged by
//! `"type"`. Binary fields are base64; a CLSID is the 32 hex digits of its bytes in compound-file
//! order.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The protocol version this editor speaks.
pub const PROTOCOL: u32 = 1;

/// A class id, in the byte order of a compound file's directory entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clsid(pub [u8; 16]);

impl Serialize for Clsid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
}

impl<'de> Deserialize<'de> for Clsid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let mut out = [0u8; 16];
        let ok = text.len() == 32
            && text.is_ascii()
            && out.iter_mut().enumerate().all(|(i, b)| match u8::from_str_radix(&text[2 * i..2 * i + 2], 16) {
                Ok(v) => {
                    *b = v;
                    true
                }
                Err(_) => false,
            });
        if ok { Ok(Clsid(out)) } else { Err(serde::de::Error::custom("a CLSID is 32 hex digits")) }
    }
}

mod b64 {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        STANDARD.decode(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

mod b64_opt {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => s.serialize_str(&STANDARD.encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(text) => STANDARD.decode(text).map(Some).map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// One stream of an object, by its `/`-separated path from the object's root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OleStream {
    pub path: String,
    #[serde(with = "b64")]
    pub bytes: Vec<u8>,
}

/// What of an embedded object an editor sees: its class and content streams (never the
/// container's own `\x01…` streams, which ibisOffice keeps).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OleContent {
    pub clsid: Clsid,
    pub prog_id: String,
    #[serde(default)]
    pub user_type: String,
    pub streams: Vec<OleStream>,
}

impl OleContent {
    pub fn stream(&self, path: &str) -> Option<&[u8]> {
        self.streams.iter().find(|s| s.path == path).map(|s| s.bytes.as_slice())
    }

    pub fn set_stream(&mut self, path: &str, bytes: Vec<u8>) {
        match self.streams.iter_mut().find(|s| s.path == path) {
            Some(s) => s.bytes = bytes,
            None => self.streams.push(OleStream { path: path.to_string(), bytes }),
        }
    }
}

/// The picture of an object: a PNG, an SVG, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    #[serde(default, with = "b64_opt", skip_serializing_if = "Option::is_none")]
    pub png: Option<Vec<u8>>,
    #[serde(default, with = "b64_opt", skip_serializing_if = "Option::is_none")]
    pub svg: Option<Vec<u8>>,
}

/// From ibisOffice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HostMsg {
    Open { content: OleContent, title: String, locale: String },
    Close,
    Ping,
    New,
    Import {
        extension: String,
        #[serde(with = "b64")]
        data: Vec<u8>,
    },
    Export { content: OleContent, extension: String },
    Render { content: OleContent },
}

impl HostMsg {
    /// The message in a frame; `None` for a type this version does not know (a newer host's),
    /// which is passed over rather than treated as an error.
    pub fn from_json(json: &[u8]) -> Result<Option<HostMsg>, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_slice(json)?;
        const KNOWN: [&str; 7] = ["Open", "Close", "Ping", "New", "Import", "Export", "Render"];
        match value.get("type").and_then(|t| t.as_str()) {
            Some(t) if KNOWN.contains(&t) => serde_json::from_value(value).map(Some),
            _ => Ok(None),
        }
    }
}

/// To ibisOffice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EditorMsg {
    Hello { token: String, protocol: u32 },
    Update {
        content: OleContent,
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },
    Closed,
    Created {
        content: OleContent,
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },
    Exported {
        #[serde(with = "b64")]
        data: Vec<u8>,
    },
    Rendered {
        preview: Preview,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size_emu: Option<(i64, i64)>,
    },
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_messages_read_as_ibis_office_writes_them() {
        let open = json!({
            "type": "Open",
            "content": {
                "clsid": "216dba412ea0ce118fd90020afd1f20c",
                "prog_id": "ChemDraw_x64.Document.6.0",
                "user_type": "CS ChemDraw 64-bit Drawing",
                "streams": [{"path": "CONTENTS", "bytes": "VmpDRA=="}]
            },
            "title": "sample.pptx",
            "locale": "ja"
        });
        let Some(HostMsg::Open { content, title, locale }) = HostMsg::from_json(open.to_string().as_bytes()).unwrap() else {
            panic!("an Open")
        };
        assert_eq!(content.clsid.0[..4], [0x21, 0x6d, 0xba, 0x41]);
        assert_eq!(content.stream("CONTENTS"), Some(&b"VjCD"[..]));
        assert_eq!((title.as_str(), locale.as_str()), ("sample.pptx", "ja"));

        let import = br#"{"type":"Import","extension":"cdx","data":"AAH/"}"#;
        assert_eq!(HostMsg::from_json(import).unwrap(), Some(HostMsg::Import { extension: "cdx".into(), data: vec![0, 1, 255] }));
        assert_eq!(HostMsg::from_json(br#"{"type":"Close"}"#).unwrap(), Some(HostMsg::Close));
        assert_eq!(HostMsg::from_json(br#"{"type":"SomethingNewer","x":1}"#).unwrap(), None);
        assert!(HostMsg::from_json(br#"{"type":"Open"}"#).is_err());
    }

    #[test]
    fn editor_messages_have_the_wire_shape() {
        let hello = serde_json::to_value(EditorMsg::Hello { token: "ab".into(), protocol: PROTOCOL }).unwrap();
        assert_eq!(hello, json!({"type": "Hello", "token": "ab", "protocol": 1}));
        let rendered = serde_json::to_value(EditorMsg::Rendered { preview: Preview { png: Some(vec![1, 2]), svg: None }, size_emu: Some((10, 20)) }).unwrap();
        assert_eq!(rendered, json!({"type": "Rendered", "preview": {"png": "AQI="}, "size_emu": [10, 20]}));
        let closed = serde_json::to_value(EditorMsg::Closed).unwrap();
        assert_eq!(closed, json!({"type": "Closed"}));
        let content = OleContent { clsid: Clsid([0xab; 16]), ..Default::default() };
        let created = serde_json::to_value(EditorMsg::Created { content, preview: Preview::default(), size_emu: None }).unwrap();
        assert_eq!(created["content"]["clsid"], json!("abababababababababababababababab"));
        assert!(created.get("size_emu").is_none());
    }
}
