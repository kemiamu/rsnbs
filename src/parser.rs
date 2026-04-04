use super::{BinaryMut, Header, Instrument, Layer, Note, Song};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};

/// Read a specified number of bytes from the reader
fn read_bytes<R: Read>(mut reader: R, byte_count: u8) -> Option<Vec<u8>> {
    let mut bytes = vec![0u8; byte_count as usize];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

trait FromBytes<T> {
    fn from_bytes(bytes: Vec<u8>) -> T;
}
impl FromBytes<i16> for i16 {
    fn from_bytes(bytes: Vec<u8>) -> i16 {
        return unsafe { std::ptr::read(bytes.as_ptr() as *const _) };
    }
}
impl FromBytes<i32> for i32 {
    fn from_bytes(bytes: Vec<u8>) -> i32 {
        return unsafe { std::ptr::read(bytes.as_ptr() as *const _) };
    }
}

/// Read a string from the reader (length-prefixed)
fn read_string<R: Read>(mut reader: R) -> Option<String> {
    let len = i32::from_bytes(read_bytes(&mut reader, 4)?);
    let mut string_bytes = vec![0u8; len as usize];
    reader.read_exact(&mut string_bytes).ok()?;
    String::from_utf8(string_bytes).ok()
}

/// Read a binary value based on its type
fn read_binary<R: Read>(mut reader: R, binary: BinaryMut) -> Option<()> {
    match binary {
        BinaryMut::Bool(bool) => {
            *bool = Some(match read_bytes(&mut reader, 1)?[0] {
                1 => true,
                0 => false,
                _ => false,
            });
        }
        BinaryMut::Byte(byte) => {
            *byte = Some(read_bytes(&mut reader, 1)?[0] as i8);
        }
        BinaryMut::UByte(ubyte) => {
            *ubyte = Some(read_bytes(&mut reader, 1)?[0]);
        }
        BinaryMut::Short(short) => {
            *short = Some(i16::from_bytes(read_bytes(&mut reader, 2)?));
        }
        BinaryMut::Integer(integer) => {
            *integer = Some(i32::from_bytes(read_bytes(&mut reader, 4)?));
        }
        BinaryMut::String(string) => {
            *string = read_string(&mut reader);
        }
    }
    Some(())
}

/// Read a part of NBS data (header, layer, or instrument)
fn read_nbs_part<R: Read>(mut reader: R, part: Vec<(BinaryMut, u8)>) -> Option<()> {
    for (binary, _version) in part {
        read_binary(&mut reader, binary);
    }
    Some(())
}

/// Internal function to read NBS data from any Read + Seek source
fn read_nbs_internal<R: Read + Seek>(mut reader: R) -> Option<Song> {
    let mut song = Song::default();

    let version_bytes = read_bytes(&mut reader, 2)?;
    if i16::from_bytes(version_bytes) <= 0i16 {
        let version_byte = read_bytes(&mut reader, 1)?;
        song.header.version = Some(version_byte[0] as i8);
    } else {
        song.header.version = Some(0i8);
        reader.seek(SeekFrom::Start(0)).ok()?;
    }

    let version = song.header.version.clone()? as u8;
    reader.seek(SeekFrom::Start(0)).ok()?;

    read_nbs_part(&mut reader, song.header.as_mut_vec(version));
    song.header.version = Some(version as i8);

    if song.header.classic_length? > 0 {
        song.header.song_length = song.header.classic_length.clone()
    }

    let mut note = Note::default();
    let mut i = 0i8;
    let mut layer = -1i32;
    let mut tick = -1i32;

    loop {
        match i {
            0 => {
                let tick_change = i16::from_bytes(read_bytes(&mut reader, 2)?);
                tick += tick_change as i32;
                note.tick = Some(tick);
                if tick_change == 0 {
                    break;
                }
            }
            1 => {
                let layer_change = i16::from_bytes(read_bytes(&mut reader, 2)?);
                layer += layer_change as i32;
                note.layer = Some(layer);
                if layer_change == 0 {
                    i = -1;
                    layer = -1;
                } else {
                    note.tick = Some(tick);
                }
            }
            2 => {
                note.instrument = Some(read_bytes(&mut reader, 1)?[0] as i8);
            }
            3 => {
                note.key = Some(read_bytes(&mut reader, 1)?[0] as i8);
                if version < 4 {
                    i = 6;
                }
            }
            4 => {
                note.velocity = Some(read_bytes(&mut reader, 1)?[0] as i8);
            }
            5 => {
                note.panning = Some(read_bytes(&mut reader, 1)?[0]);
            }
            6 => {
                note.pitch = Some(i16::from_bytes(read_bytes(&mut reader, 2)?));
            }
            _ => {
                panic!(":skull:")
            }
        }

        if i == 6 {
            i = 1;
            song.notes.push(note);
            note = Note::default();
        } else {
            i += 1;
        }
    }

    song.layers = vec![];
    let layers = song.header.song_layers.clone();
    for _ in 0..layers? {
        let mut layer = Layer::default();
        read_nbs_part(&mut reader, layer.as_mut_vec(version));
        song.layers.push(layer);
    }

    let instruments = read_bytes(&mut reader, 1)?[0];

    for _ in 0..instruments {
        let mut instrument = Instrument::default();
        read_nbs_part(&mut reader, instrument.as_mut_vec(version));
        song.instruments.push(instrument);
    }

    Some(song)
}

/// Read NBS data from a file
pub fn read_nbs(filepath: &str) -> Option<Song> {
    let file = File::open(filepath).ok()?;
    read_nbs_internal(file)
}

/// Read NBS data from standard input
pub fn read_nbs_from_stdin() -> Option<Song> {
    use std::io::{stdin, Read};

    let mut buffer = Vec::new();
    stdin().read_to_end(&mut buffer).ok()?;

    let cursor = Cursor::new(buffer);
    read_nbs_internal(cursor)
}
