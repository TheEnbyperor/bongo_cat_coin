extern crate sha2;
extern crate chrono;
use sha2::{Sha512, Digest};
use chrono::prelude::*;

pub type Sha512Hash = Vec<u8>;

trait BlockData {
    fn data(&self) -> Vec<u8>;
    fn box_clone(&self) -> Box<BlockData>;
}


impl Clone for Box<BlockData> {
    fn clone(&self) -> Box<BlockData> {
        self.box_clone()
    }
}

struct Block {
    id: u64,
    timestamp: i64,
    nonce: u64,
    prev_block_hash: Sha512Hash,
    data: Vec<Box<BlockData>>,
}

impl Block {
    fn headers(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(&convert_u64_to_u8_array(self.id));
        vec.extend(&convert_u64_to_u8_array(self.nonce));
        vec.extend(&convert_u64_to_u8_array(self.timestamp as u64));
        vec.extend_from_slice(&self.prev_block_hash);
        vec
    }

    fn hash(&self) -> Sha512Hash {
        let mut hasher = Sha512::default();

        hasher.input(&self.headers());

        for elm in self.data.iter() {
            hasher.input(&elm.data());
        }

        hasher.result().as_slice().to_owned()
    }

    pub fn new(data: &Vec<Box<BlockData>>, prev_block_hash: Sha512Hash, id: u64) -> Self {
        Self {
            id,
            prev_block_hash,
            timestamp: Utc::now().timestamp(),
            nonce: 0,
            data: data.to_owned().to_vec(),
        }
    }

    pub fn genesis() -> Self {
        Self::new(&vec![BinaryData::new(&b"Genesis block".to_vec()).box_clone()],
                  Sha512Hash::default(), 0)
    }
}

pub fn convert_u64_to_u8_array(val: u64) -> [u8; 8] {
    return [
        val as u8,
        (val >> 8) as u8,
        (val >> 16) as u8,
        (val >> 24) as u8,
        (val >> 32) as u8,
        (val >> 40) as u8,
        (val >> 48) as u8,
        (val >> 56) as u8,
    ]
}

#[derive(Clone)]
struct Transaction {
    sender: u64,
    recipient: u64,
    amount: u64,
}

impl BlockData for Transaction {
    fn data(&self) -> Vec<u8> {
        let mut data = vec![1 as u8];

        data.extend_from_slice(&convert_u64_to_u8_array(self.sender));
        data.extend_from_slice(&convert_u64_to_u8_array(self.recipient));
        data.extend_from_slice(&convert_u64_to_u8_array(self.amount));

        data
    }
    fn box_clone(&self) -> Box<BlockData> {
        Box::new((*self).clone())
    }
}

#[derive(Clone)]
struct BinaryData {
    data: Vec<u8>
}

impl BinaryData {
    pub fn new(data: &Vec<u8>) -> Self {
        Self {
            data: data.to_owned().to_vec(),
        }
    }
}

impl BlockData for BinaryData {
    fn data(&self) -> Vec<u8> {
        let mut data = vec![0 as u8];

        data.extend(self.data.to_vec());

        data
    }
    fn box_clone(&self) -> Box<BlockData> {
        Box::new((*self).clone())
    }
}

pub struct Blockchain {
    blocks: Vec<Block>,
}

impl Blockchain {
    // Initializes a new blockchain with a genesis block.
    pub fn new() -> Self {
        let blocks = Block::genesis();

        Self {
            blocks: vec![blocks]
        }
    }
}

fn main() {
    println!("#BongoCatCoin");
}