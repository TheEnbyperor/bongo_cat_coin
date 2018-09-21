//! Automatically generated rust module for 'chain.proto' file

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use std::io::Write;
use std::borrow::Cow;
use quick_protobuf::{MessageRead, MessageWrite, BytesReader, Writer, Result};
use quick_protobuf::sizeofs::*;
use super::*;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Transaction<'a> {
    pub from: Cow<'a, [u8]>,
    pub to: Cow<'a, [u8]>,
    pub amount: u64,
}

impl<'a> MessageRead<'a> for Transaction<'a> {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.from = r.read_bytes(bytes).map(Cow::Borrowed)?,
                Ok(18) => msg.to = r.read_bytes(bytes).map(Cow::Borrowed)?,
                Ok(24) => msg.amount = r.read_uint64(bytes)?,
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl<'a> MessageWrite for Transaction<'a> {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.from).len())
        + 1 + sizeof_len((&self.to).len())
        + 1 + sizeof_varint(*(&self.amount) as u64)
    }

    fn write_message<W: Write>(&self, w: &mut Writer<W>) -> Result<()> {
        w.write_with_tag(10, |w| w.write_bytes(&**&self.from))?;
        w.write_with_tag(18, |w| w.write_bytes(&**&self.to))?;
        w.write_with_tag(24, |w| w.write_uint64(*&self.amount))?;
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct BinaryData<'a> {
    pub data: Cow<'a, [u8]>,
}

impl<'a> MessageRead<'a> for BinaryData<'a> {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.data = r.read_bytes(bytes).map(Cow::Borrowed)?,
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl<'a> MessageWrite for BinaryData<'a> {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_len((&self.data).len())
    }

    fn write_message<W: Write>(&self, w: &mut Writer<W>) -> Result<()> {
        w.write_with_tag(10, |w| w.write_bytes(&**&self.data))?;
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Block<'a> {
    pub id: u64,
    pub timestamp: i64,
    pub nonce: u64,
    pub prev_block_hash: Cow<'a, [u8]>,
    pub data: Vec<mod_Block::Data<'a>>,
}

impl<'a> MessageRead<'a> for Block<'a> {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.id = r.read_uint64(bytes)?,
                Ok(16) => msg.timestamp = r.read_int64(bytes)?,
                Ok(24) => msg.nonce = r.read_uint64(bytes)?,
                Ok(34) => msg.prev_block_hash = r.read_bytes(bytes).map(Cow::Borrowed)?,
                Ok(42) => msg.data.push(r.read_message::<mod_Block::Data>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl<'a> MessageWrite for Block<'a> {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_varint(*(&self.id) as u64)
        + 1 + sizeof_varint(*(&self.timestamp) as u64)
        + 1 + sizeof_varint(*(&self.nonce) as u64)
        + 1 + sizeof_len((&self.prev_block_hash).len())
        + self.data.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
    }

    fn write_message<W: Write>(&self, w: &mut Writer<W>) -> Result<()> {
        w.write_with_tag(8, |w| w.write_uint64(*&self.id))?;
        w.write_with_tag(16, |w| w.write_int64(*&self.timestamp))?;
        w.write_with_tag(24, |w| w.write_uint64(*&self.nonce))?;
        w.write_with_tag(34, |w| w.write_bytes(&**&self.prev_block_hash))?;
        for s in &self.data { w.write_with_tag(42, |w| w.write_message(s))?; }
        Ok(())
    }
}

pub mod mod_Block {

use super::*;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Data<'a> {
    pub type_pb: mod_Block::DataType,
    pub transaction: Option<Transaction<'a>>,
    pub binaryData: Option<BinaryData<'a>>,
}

impl<'a> MessageRead<'a> for Data<'a> {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.type_pb = r.read_enum(bytes)?,
                Ok(18) => msg.transaction = Some(r.read_message::<Transaction>(bytes)?),
                Ok(26) => msg.binaryData = Some(r.read_message::<BinaryData>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl<'a> MessageWrite for Data<'a> {
    fn get_size(&self) -> usize {
        0
        + 1 + sizeof_varint(*(&self.type_pb) as u64)
        + self.transaction.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
        + self.binaryData.as_ref().map_or(0, |m| 1 + sizeof_len((m).get_size()))
    }

    fn write_message<W: Write>(&self, w: &mut Writer<W>) -> Result<()> {
        w.write_with_tag(8, |w| w.write_enum(*&self.type_pb as i32))?;
        if let Some(ref s) = self.transaction { w.write_with_tag(18, |w| w.write_message(s))?; }
        if let Some(ref s) = self.binaryData { w.write_with_tag(26, |w| w.write_message(s))?; }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DataType {
    BINARY_DATA = 0,
    TRANSACTION = 1,
}

impl Default for DataType {
    fn default() -> Self {
        DataType::BINARY_DATA
    }
}

impl From<i32> for DataType {
    fn from(i: i32) -> Self {
        match i {
            0 => DataType::BINARY_DATA,
            1 => DataType::TRANSACTION,
            _ => Self::default(),
        }
    }
}

impl<'a> From<&'a str> for DataType {
    fn from(s: &'a str) -> Self {
        match s {
            "BINARY_DATA" => DataType::BINARY_DATA,
            "TRANSACTION" => DataType::TRANSACTION,
            _ => Self::default(),
        }
    }
}

}

