//! Provides extension traits for reading and writing NBS format data.

use encoding_rs::WINDOWS_1252;
use std::{io, num::NonZeroU32};

/// Provides methods for reading NBS format data (little-endian) for Read types.
pub trait NbsReadExt: io::Read {
    /// Reads a bool (stored as u8 where 1 = true, 0 = false).
    fn read_bool(&mut self) -> io::Result<bool> {
        let value = self.read_u8()?;
        Ok(value == 1)
    }

    /// Reads a u8.
    fn read_u8(&mut self) -> io::Result<u8> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    /// Reads a u16 (little-endian).
    fn read_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Reads an i16 (little-endian).
    fn read_i16(&mut self) -> io::Result<i16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(i16::from_le_bytes(buf))
    }

    /// Reads a u32 (little-endian).
    fn read_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Reads a length-prefixed string.
    fn read_string(&mut self) -> io::Result<String> {
        let len = self.read_u32()?;
        let mut buf = vec![0; len as usize];
        self.read_exact(&mut buf)?;
        // 可恶的欧洲人！
        // Decode Windows-1252 encoded string
        let (decoded, _, _) = WINDOWS_1252.decode(&buf);
        Ok(decoded.into())
    }

    /// Reads a jump time (stored as u16 where 0 = no jump).
    fn read_jump(&mut self) -> io::Result<Option<NonZeroU32>> {
        self.read_u16().map(|time| NonZeroU32::new(time as u32))
    }
}

impl<R: io::Read + ?Sized> NbsReadExt for R {}

/// Provides methods for writing NBS format data (little-endian) for Write types.
pub trait NbsWriteExt: io::Write {
    /// Writes a bool (stored as u8 where 1 = true, 0 = false).
    fn write_bool(&mut self, value: bool) -> io::Result<()> {
        self.write_u8(if value { 1 } else { 0 })
    }

    /// Writes a u8.
    fn write_u8(&mut self, value: u8) -> io::Result<()> {
        self.write_all(&[value])
    }

    /// Writes a u16 (little-endian).
    fn write_u16(&mut self, value: u16) -> io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    /// Writes an i16 (little-endian).
    fn write_i16(&mut self, value: i16) -> io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    /// Writes a u32 (little-endian).
    fn write_u32(&mut self, value: u32) -> io::Result<()> {
        self.write_all(&value.to_le_bytes())
    }

    /// Writes a length-prefixed string.
    fn write_string(&mut self, s: &str) -> io::Result<()> {
        // 可恶的欧洲人！
        // Encode string to Windows-1252
        let (bytes, _, _) = WINDOWS_1252.encode(s);
        let len: u32 = bytes.len().saturating_into();
        self.write_u32(len)?;
        self.write_all(&bytes[..len as usize])
    }

    /// Writes a jump time (stored as u16 where 0 = no jump).
    fn write_jump(&mut self, time: Option<NonZeroU32>) -> io::Result<()> {
        match time {
            Some(time) => self.write_u16(time.get().saturating_into()),
            None => self.write_u16(0),
        }
    }
}

impl<W: io::Write + ?Sized> NbsWriteExt for W {}

pub trait SaturatingCast<T> {
    fn saturating_into(self) -> T;
}

macro_rules! impl_saturating_cast {
    ($from:ty => $to:ty) => {
        impl SaturatingCast<$to> for $from {
            fn saturating_into(self) -> $to {
                self.try_into().unwrap_or(<$to>::MAX)
            }
        }
    };
}

impl_saturating_cast!(u32 => u8);
impl_saturating_cast!(u32 => u16);
impl_saturating_cast!(usize => u8);
impl_saturating_cast!(usize => u32);
