extern crate paste;

pub enum BinaryMut<'a> {
    Bool(&'a mut Option<bool>),
    Byte(&'a mut Option<i8>),
    UByte(&'a mut Option<u8>),
    Short(&'a mut Option<i16>),
    Integer(&'a mut Option<i32>),
    String(&'a mut Option<String>),
}

pub enum Binary<'b> {
    Bool(&'b Option<bool>),
    Byte(&'b Option<i8>),
    UByte(&'b Option<u8>),
    Short(&'b Option<i16>),
    Integer(&'b Option<i32>),
    String(&'b Option<String>),
}

macro_rules! create_iterable_struct {
    ($struct_name:ident, [$(($field:ident: $type:ty: $enum:expr): $version:expr),*]) => {
        // Define the struct
        #[derive(Debug, PartialEq)]
        pub struct $struct_name {
            $(
                pub $field: Option<$type>,
            )*
        }

        // Implement the iterator function for the struct
        impl $struct_name {
            paste::item! {
                pub fn as_mut_vec(&mut self, version: u8) -> Vec<(BinaryMut, u8)> {
                    vec![
                        $(
                            (BinaryMut::$enum(&mut self.$field), $version),
                        )*
                    ].into_iter().filter(|x| x.1 <= version).collect()
                }
                pub fn as_ref_vec(&self, version: u8) -> Vec<(Binary, u8)> {
                    vec![
                        $(
                            (Binary::$enum(&self.$field), $version),
                        )*
                    ].into_iter().filter(|x| x.1 <= version).collect()
                }
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Self {
                    $(
                        $field: None,
                    )*
                }
            }
        }
    };
    ($type:ty) => {}
}

create_iterable_struct!(
    Header,
    [
        (classic_length: i16: Short): 0,
        (version: i8: Byte): 1,
        (default_instruments: i8: Byte): 1,
        (song_length: i16: Short): 3,
        (song_layers: i16: Short): 0,
        (song_name: String: String): 0,
        (song_author: String: String): 0,
        (original_author: String: String): 0,
        (description: String: String): 0,
        (tempo: i16: Short): 0,
        (auto_save: bool: Bool): 0,
        (auto_saving_duration: i8: Byte): 0,
        (time_signature: i8: Byte): 0,
        (minutes_spent: i32: Integer): 0,
        (left_clicks: i32: Integer): 0,
        (right_clicks: i32: Integer): 0,
        (blocks_added: i32: Integer): 0,
        (blocks_removed: i32: Integer): 0,
        (og_file: String: String): 0,
        (r#loop: bool: Bool): 4,
        (max_loop_count: i8: Byte): 4,
        (loop_start: i16: Short): 4
    ]
);

#[derive(Debug)]
pub struct Note {
    pub tick: Option<i32>,
    pub layer: Option<i32>,
    pub instrument: Option<i8>,
    pub key: Option<i8>,
    pub velocity: Option<i8>,
    pub panning: Option<u8>,
    pub pitch: Option<i16>,
}

// Implement PartialEq separately, ignoring tick and layer fields, i.e., ignoring the note's position
impl PartialEq for Note {
    fn eq(&self, other: &Self) -> bool {
        self.instrument == other.instrument
            && self.key == other.key
            && self.velocity == other.velocity
            && self.panning == other.panning
            && self.pitch == other.pitch
    }
}

impl Note {
    paste::item! {
        pub fn as_mut_vec(&mut self, version: u8) -> Vec<(BinaryMut, u8)> {
            vec![
                (BinaryMut::Integer(&mut self.tick), 127),
                (BinaryMut::Integer(&mut self.layer), 127),
                (BinaryMut::Byte(&mut self.instrument), 0),
                (BinaryMut::Byte(&mut self.key), 0),
                (BinaryMut::Byte(&mut self.velocity), 4),
                (BinaryMut::UByte(&mut self.panning), 4),
                (BinaryMut::Short(&mut self.pitch), 4),
            ].into_iter().filter(|x| x.1 <= version).collect()
        }
        pub fn as_ref_vec(&self, version: u8) -> Vec<(Binary, u8)> {
            vec![
                (Binary::Integer(&self.tick), 127),
                (Binary::Integer(&self.layer), 127),
                (Binary::Byte(&self.instrument), 0),
                (Binary::Byte(&self.key), 0),
                (Binary::Byte(&self.velocity), 4),
                (Binary::UByte(&self.panning), 4),
                (Binary::Short(&self.pitch), 4),
            ].into_iter().filter(|x| x.1 <= version).collect()
        }
    }
}

impl Default for Note {
    fn default() -> Self {
        Self {
            tick: None,
            layer: None,
            instrument: None,
            key: None,
            velocity: None,
            panning: None,
            pitch: None,
        }
    }
}

create_iterable_struct!(
    Layer,
    [
        (name: String: String): 0,
        (lock: bool: Bool): 4,
        (volume: i8: Byte): 0,
        (stereo: u8: UByte): 2
    ]
);

create_iterable_struct!(
    Instrument,
    [
        (name: String: String): 0,
        (file: String: String): 0,
        (pitch: i8: Byte): 0,
        (press_key: bool: Bool): 0
    ]
);
