mod fields;
mod tests;
mod writer;

pub mod parser;
pub use fields::{Binary, BinaryMut, Header, Instrument, Layer, Note};
pub use parser::{read_nbs, read_nbs_from_stdin};

#[derive(Debug, PartialEq)]
pub struct Song {
    pub header: Header,
    pub notes: Vec<Note>,
    pub layers: Vec<Layer>,
    pub instruments: Vec<Instrument>,
}

impl Default for Song {
    fn default() -> Self {
        return Song {
            header: Header::default(),
            notes: vec![],
            layers: vec![Layer {
                name: Some(String::new()),
                lock: Some(false),
                volume: Some(1),
                stereo: Some(0),
            }],
            instruments: vec![],
        };
    }
}
