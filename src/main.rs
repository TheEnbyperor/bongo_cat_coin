extern crate chrono;
extern crate crypto;
#[macro_use]
extern crate juniper;
extern crate juniper_warp;
extern crate warp;
extern crate hex;
extern crate quick_protobuf;
extern crate regex;

mod proto;

use warp::Filter;
use crypto::sha2::Sha256;
use crypto::digest::Digest;
use chrono::prelude::*;
use std::fmt;
use std::thread;
use std::fs;
use std::io;
use std::path;
use std::sync::{Arc, RwLock};
use std::borrow::Cow;
use juniper::FieldResult;
use proto::chain;
use regex::Regex;

pub type Sha256Hash = [u8; 32];

fn sha256hash_from_slice(bytes: &[u8]) -> Sha256Hash {
    let mut array = [0; 32];
    let bytes = &bytes[..array.len()]; // panics if not enough data
    array.copy_from_slice(bytes);
    array
}

trait BlockData: Sync + Send {
    fn data(&self) -> Vec<u8>;
    fn box_clone(&self) -> Box<BlockData>;
    fn debug(&self, f: &mut fmt::Formatter)-> fmt::Result;

    fn as_binary_data(&self) -> Option<&BinaryData> { None }
    fn as_transaction(&self) -> Option<&Transaction> { None }
}

graphql_union!(<'a> &'a BlockData: () as "BlockData" |&self| {
    instance_resolvers: |_| {
        &BinaryData => self.as_binary_data(),
        &Transaction => self.as_transaction(),
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
    data: Vec<Box<BlockData>>
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
        hex::encode_upper(&self.prev_block_hash)
    }

    field data() -> &BlockData {
        Box::leak(self.data[0].clone())
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
        Self::new(&vec![BinaryData::new(&b"Genesis block".to_vec())],
                  Sha256Hash::default(), 0)
    }

    fn next_block(&self) -> Self {
        let next_block = self.id + 1;
        Self::new(&vec![BinaryData::new(&format!("Block {}", next_block).as_bytes().to_vec())],
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

    fn as_transaction(&self) -> Option<&Transaction> { Some(&self) }
}

graphql_object!(Transaction: () |&self|{
    field sender() -> String {
        hex::encode_upper(&self.sender)
    }

    field recipient() -> String {
        hex::encode_upper(&self.sender)
    }

    field amount() -> i32 {
        self.amount as i32
    }

    field data_type() -> &str {
        "transaction"
    }
});

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

    fn as_binary_data(&self) -> Option<&BinaryData> { Some(&self) }
}

graphql_object!(BinaryData: () |&self|{
    field data() -> String {
        hex::encode(&self.data)
    }

    field data_type() -> &str {
        "binary"
    }
});

#[derive(Debug)]
struct Blockchain {
    blocks: Vec<Block>,
    pending_data: Vec<Box<BlockData>>,
}

impl Blockchain {
    // Initializes a new blockchain with a genesis block.
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            pending_data: vec![],
        }
    }

    fn init_genesis(&mut self) {
        self.blocks.push(Block::genesis());
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

    fn add_data(&mut self, data: Box<BlockData>) {
        self.pending_data.push(data);
    }
}

struct Context {
    blockchain: Arc<RwLock<Blockchain>>,
}
impl juniper::Context for Context {}

struct Query;

graphql_object!(Query: Context |&self| {
    field block(&executor, id: i32) -> FieldResult<Block> {
        let context = executor.context();
        let chain = context.blockchain.read().unwrap();
        match chain.blocks.get(id as usize) {
            Some(block) => {
                Ok(block.clone())
            }
            None => {
                Err(juniper::FieldError::new("Block does not exist", graphql_value!(None)))
            }
        }
    }

    field blocks(&executor, start: i32, len: i32) -> FieldResult<Vec<Block>> {
        let context = executor.context();
        let chain = context.blockchain.read().unwrap();
        if (start+len) as usize > chain.blocks.len() {
            return Err(juniper::FieldError::new("Block does not exist", graphql_value!(None)))
        }
        Ok(chain.blocks[start as usize..(start+len) as usize].iter().cloned().collect())
    }
});

struct Mutation;

graphql_object!(Mutation: Context |&self| {
    field publishTransaction(&executor, from: String, to: String, amount: i32) -> FieldResult<Transaction> {
        let from_vec: Vec<u8>;
        let to_vec: Vec<u8>;

        let context = executor.context();
        let mut chain = context.blockchain.write().unwrap();

        match hex::decode(from) {
            Ok(data) => {
                from_vec = data;
            }
            Err(e) => {
                return Err(juniper::FieldError::new("Invalid hex from address", graphql_value!(None)));
            }
        }
        match hex::decode(to) {
            Ok(data) => {
                to_vec = data;
            }
            Err(e) => {
                return Err(juniper::FieldError::new("Invalid hex to address", graphql_value!(None)));
            }
        }

        if from_vec.len() != 32 {
            return Err(juniper::FieldError::new("Invalid length from address", graphql_value!(None)));
        }
        if to_vec.len() != 32 {
            return Err(juniper::FieldError::new("Invalid length to address", graphql_value!(None)));
        }

        let transaction = Transaction {
            sender: sha256hash_from_slice(&from_vec),
            recipient: sha256hash_from_slice(&to_vec),
            amount: amount as u64,
        };

        chain.add_data(Box::new(transaction.clone()));

        Ok(transaction)
    }
});

type Schema = juniper::RootNode<'static, Query, Mutation>;

fn schema() -> Schema {
    Schema::new(Query, Mutation)
}

fn block_to_pb(block: &Block) -> chain::Block {
    let mut block_data: Vec<chain::mod_Block::Data> = vec![];
    for data in block.data.iter() {
        match data.as_binary_data() {
            Some(data) => {
                block_data.push(chain::mod_Block::Data {
                    type_pb: chain::mod_Block::DataType::BINARY_DATA,
                    binaryData: Some(chain::BinaryData {
                        data: Cow::Borrowed(&data.data[..])
                    }),
                    transaction: None,
                });
                continue;
            }
            None => {}
        };
        match data.as_transaction() {
            Some(data) => {
                block_data.push(chain::mod_Block::Data {
                    type_pb: chain::mod_Block::DataType::TRANSACTION,
                    transaction: Some(chain::Transaction {
                        from: Cow::Borrowed(&data.sender[..]),
                        to: Cow::Borrowed(&data.recipient[..]),
                        amount: data.amount,
                    }),
                    binaryData: None,
                });
                continue;
            }
            None => {}
        };
    }

    chain::Block {
        id: block.id,
        timestamp: block.timestamp,
        nonce: block.nonce,
        prev_block_hash: Cow::Borrowed(&block.prev_block_hash[..]),
        data: block_data,
    }
}

fn write_pb_block(block: &chain::Block) -> Result<(), quick_protobuf::Error> {
    match fs::File::create(format!("./blocks/block{}", block.id)) {
        Ok(mut out) => {
            let mut writer = quick_protobuf::Writer::new(&mut out);
            writer.write_message(block)
        }
        Err(e) => {
            Err(quick_protobuf::Error::Io(e))
        }
    }
}

fn start_db_thread(blockchain: Arc<RwLock<Blockchain>>) {
    thread::spawn(move|| {
        for block in blockchain.read().unwrap().blocks.iter() {
            let block_msg = block_to_pb(block);
            match write_pb_block(&block_msg) {
                Ok(_) => {}
                Err(e) => {
                    panic!("Failed to write block to fs: {}", e)
                }
            }
        }
    });
}

fn init_db() -> io::Result<Blockchain>{
    let mut chain = Blockchain::new();

    let re = Regex::new(r"^block\d+$").unwrap();

    let db_path = path::Path::new("./blocks");
    fs::create_dir_all(db_path)?;
    let dir = fs::read_dir(db_path)?;
    let mut files: Vec<_> = vec![];
    for entry in dir {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            match path.file_name() {
                None => {}
                Some(file_name) => {
                    match file_name.to_str() {
                        None => {}
                        Some(file_name) => {
                            if re.is_match(file_name) {
                                files.push(path);
                            }
                        }
                    }
                }
            }
        }
    };

    for block in files.iter() {
        let reader = quick_protobuf::Reader::from_file(block)?;
    }

    let smallest_block = -1;
    let largest_block = -1;

    if smallest_block != 0 {
        chain.init_genesis();
    }

    Ok(chain)
}

fn main() {
    let log = warp::log("warp_server");

    let chain;
    match init_db() {
        Ok(blockchain) => {
            chain = Arc::new(RwLock::new(blockchain));
        }
        Err(e) => {
            panic!("Cannot initalise chain: {}", e);
        }
    }

    chain.write().unwrap().add_block();

    start_db_thread(chain.clone());

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