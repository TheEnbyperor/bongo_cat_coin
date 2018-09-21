extern crate chrono;
extern crate crypto;
#[macro_use]
extern crate juniper;
extern crate juniper_warp;
extern crate warp;
use warp::Filter;
use crypto::sha2::Sha256;
use crypto::digest::Digest;
use chrono::prelude::*;
use std::fmt;
use std::sync::{Arc, Mutex};

pub type Sha256Hash = [u8; 32];

trait BlockData: Sync + Send {
    fn data(&self) -> Vec<u8>;
    fn box_clone(&self) -> Box<BlockData>;
    fn debug(&self, f: &mut fmt::Formatter)-> fmt::Result;
}

graphql_object!(BlockData: () |&self|{
    field data() -> Vec<u8> {
        self.data()
    }
});

impl Clone for Box<BlockData> {
    fn clone(&self) -> Box<BlockData> {
        self.box_clone()
    }
}

impl fmt::Debug for Box<BlockData> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.debug(f)
    }
}

#[derive(Debug, Clone)]
struct Block {
    id: u64,
    timestamp: i64,
    nonce: u64,
    prev_block_hash: Sha256Hash,
    data: Vec<Box<BlockData>>,
}

graphql_object!(Block: () |&self|{
    field id() -> i32 {
        self.id as i32
    }

    field timestamp() -> i32 {
        self.timestamp as i32
    }

    field nonce() -> i32 {
        self.nonce as i32
    }

    field prev_block_hash() -> String {
        let strs: Vec<String> =
          self.prev_block_hash.iter()
                               .map(|b| format!("{:02X}", b))
                               .collect();
        strs.join("")
    }

    field data() -> Vec<Box<BlockData>> {
        self.data
    }
});

impl Block {
    fn headers(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(&convert_u64_to_u8_array(self.id));
        vec.extend(&convert_u64_to_u8_array(self.nonce));
        vec.extend(&convert_u64_to_u8_array(self.timestamp as u64));
        vec.extend_from_slice(&self.prev_block_hash);
        vec
    }

    fn hash(&self) -> Sha256Hash {
        let mut hasher = Sha256::new();
        let mut hash = Sha256Hash::default();

        hasher.input(&self.headers());

        for elm in self.data.iter() {
            hasher.input(&elm.data());
        }

        hasher.result(&mut hash);
        hash
    }

    pub fn new(data: &Vec<Box<BlockData>>, prev_block_hash: Sha256Hash, id: u64) -> Self {
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
                  Sha256Hash::default(), 0)
    }

    fn next_block(&self) -> Self {
        let next_block = self.id + 1;
        Self::new(&vec![BinaryData::new(&format!("Block {}", next_block).as_bytes().to_vec()).box_clone()],
                  self.hash(), next_block)
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

#[derive(Debug, Clone)]
struct Transaction {
    sender: Sha256Hash,
    recipient: Sha256Hash,
    amount: u64,
}

impl BlockData for Transaction {
    fn data(&self) -> Vec<u8> {
        let mut data = vec![1 as u8];

        data.extend_from_slice(&self.sender);
        data.extend_from_slice(&self.recipient);
        data.extend_from_slice(&convert_u64_to_u8_array(self.amount));

        data
    }
    fn box_clone(&self) -> Box<BlockData> {
        Box::new((*self).clone())
    }
    fn debug(&self, f: &mut fmt::Formatter)-> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
struct BinaryData {
    data: Vec<u8>
}

impl BinaryData {
    pub fn new(data: &Vec<u8>) -> Box<BlockData> {
        std::boxed::Box::new(Self {
            data: data.to_owned().to_vec(),
        })
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
    fn debug(&self, f: &mut fmt::Formatter)-> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
struct Blockchain {
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

    fn add_block(&mut self) {
        let block: Block;
        match self.blocks.last() {
            Some(last_block) => {
                block = last_block.next_block();
            }
            None => {
                println!("No parent");
                return;
            }
        }
        self.blocks.push(block);
    }
}

struct Context {
    blockchain: Arc<Mutex<Blockchain>>,
}
impl juniper::Context for Context {}

struct Query;

graphql_object!(Query: Context |&self| {
    field block(&executor, id: i32) -> juniper::FieldResult<Block> {
        let context = executor.context();
        Ok(context.blockchain.lock().unwrap().blocks[id as usize].clone())
    }
});

type Schema = juniper::RootNode<'static, Query, juniper::EmptyMutation<Context>>;

fn schema() -> Schema {
    Schema::new(Query, juniper::EmptyMutation::new())
}

fn main() {
    let log = warp::log("warp_server");

    let chain = Arc::new(Mutex::new(Blockchain::new()));
    chain.lock().unwrap().add_block();

    let state = warp::any().map(move || Context {
        blockchain: chain.clone(),
    });
    let graphql_filter = juniper_warp::make_graphql_filter(schema(), state.boxed());

    warp::serve(
        warp::get2()
            .and(warp::path("graphql"))
            .and(juniper_warp::graphiql_handler("/graphql"))
            .or(warp::path("graphql").and(graphql_filter))
            .with(log),
    ).run(([127,0,0,1], 3000));
}