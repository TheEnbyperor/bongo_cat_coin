extern crate chrono;
extern crate crypto;
#[macro_use]
extern crate juniper;
extern crate juniper_warp;
extern crate warp;
extern crate hex;
extern crate protobuf;
extern crate regex;
extern crate num_bigint;
extern crate num_traits;
extern crate num_cpus;
extern crate timer;
extern crate ctrlc;

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
use std::collections::HashMap;
use std::sync::{Arc, RwLock, mpsc};
use juniper::FieldResult;
use proto::chain;
use regex::Regex;
use protobuf::Message;
use num_bigint::BigUint;
use num_traits::One;
use timer::Timer;

pub type Sha256Hash = [u8; 32];

const GENESIS_DIFFICULTY: u8 = 0xa;

fn sha256hash_from_slice(bytes: &[u8]) -> Sha256Hash {
    let mut array = [0; 32];
    let bytes = &bytes[..array.len()]; // panics if not enough data
    array.copy_from_slice(bytes);
    array
}

trait BlockData: Sync + Send {
    fn data(&self) -> Vec<u8>;
    fn box_clone(&self) -> Box<BlockData>;
    fn debug(&self, f: &mut fmt::Formatter) -> fmt::Result;

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
struct BlockInner {
    id: u64,
    timestamp: i64,
    nonce: u64,
    prev_block_hash: Sha256Hash,
    data: Vec<Box<BlockData>>,
    prev_block_index: i64,
    next_block_indexes: Vec<i64>,
}

#[derive(Debug, Clone)]
struct Block {
    inner: Arc<RwLock<BlockInner>>
}

graphql_object!(Block: () |&self|{
    field id() -> i32 {
        self.inner.read().unwrap().id as i32
    }

    field timestamp() -> i32 {
        self.inner.read().unwrap().timestamp as i32
    }

    field nonce() -> i32 {
        self.inner.read().unwrap().nonce as i32
    }

    field prev_block_hash() -> String {
        hex::encode_upper(&self.inner.read().unwrap().prev_block_hash)
    }

    field data() -> &BlockData {
        Box::leak(self.inner.read().unwrap().data[0].clone())
    }
});

impl BlockInner {
    fn headers(&self, nonce: u64) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend(&convert_u64_to_u8_array(self.id));
        vec.extend(&convert_u64_to_u8_array(nonce));
        vec.extend(&convert_u64_to_u8_array(self.timestamp as u64));
        vec.extend_from_slice(&self.prev_block_hash);
        vec
    }

    fn hash_with_nonce(&self, nonce: u64) -> Sha256Hash {
        let mut hasher = Sha256::new();
        let mut hash = Sha256Hash::default();

        hasher.input(&self.headers(nonce));

        for elm in self.data.iter() {
            hasher.input(&elm.data());
        }

        hasher.result(&mut hash);
        hash
    }

    fn hash(&self) -> Sha256Hash {
        self.hash_with_nonce(self.nonce)
    }
}

impl Block {
    fn mine(&self, difficulty: u8) -> Option<u64> {
        let found_flag = Arc::new(RwLock::new(false));
        let nonce_out = Arc::new(RwLock::new(0 as u64));
        let target = Arc::new(BigUint::one() << (256 - difficulty as usize));
        let inner = self.inner.read().unwrap();
        let mut threads = vec![];

        println!("Started mining block #{} with difficulty {}", inner.id, difficulty);

        for _ in 0..num_cpus::get() {
            let found_flag = Arc::clone(&found_flag);
            let nonce_out = Arc::clone(&nonce_out);
            let block = Arc::clone(&self.inner);
            let target = Arc::clone(&target);
            let handle = thread::spawn(move || {
                for nonce in 0..std::u64::MAX {
                    {
                        let mut flag = found_flag.read().unwrap();
                        if *flag {
                            return;
                        }
                    }

                    {
                        let hash = (*block.read().unwrap()).hash_with_nonce(nonce);
                        let hash = BigUint::from_bytes_be(&hash);

                        if hash < *target {
                            {
                                let mut flag = found_flag.write().unwrap();
                                let mut nonce_out = nonce_out.write().unwrap();
                                *flag = true;
                                *nonce_out = nonce;
                            }
                            return;
                        }
                    }
                };
            });
            threads.push(handle);
        }

        for handle in threads {
            handle.join().unwrap();
        }

        if !*found_flag.read().unwrap() {
            println!("Giving up mining block #{}", inner.id);
            None
        } else {
            let nonce = *nonce_out.read().unwrap();
            println!("Found nonce for block #{}: {}", inner.id, nonce);
            Some(nonce)
        }
    }

    fn is_valid(&self, difficulty: u8) -> bool {
        let target = BigUint::one() << (256 - difficulty as usize);
        let inner = self.inner.read().unwrap();
        let hash = inner.hash();
        let hash = BigUint::from_bytes_be(&hash);
        hash < target
    }

    fn hash(&self) -> Sha256Hash {
        let inner = self.inner.read().unwrap();
        inner.hash()
    }

    pub fn new(data: &Vec<Box<BlockData>>, prev_block_hash: Sha256Hash, prev_block_index: i64, id: u64) -> Self {
        let inner = BlockInner {
            id,
            prev_block_hash,
            timestamp: Utc::now().timestamp(),
            nonce: 0,
            data: data.to_owned().to_vec(),
            prev_block_index,
            next_block_indexes: vec![]
        };
        Self {
            inner: Arc::new(RwLock::new(inner))
        }
    }

    pub fn restore(data: &Vec<Box<BlockData>>, prev_block_hash: Sha256Hash, id: u64, timestamp: i64,
                   nonce: u64) -> Self {
        let inner = BlockInner {
            id,
            prev_block_hash,
            timestamp,
            nonce,
            data: data.to_owned().to_vec(),
            prev_block_index: -1,
            next_block_indexes: vec![]
        };
        Self {
            inner: Arc::new(RwLock::new(inner))
        }
    }

    pub fn genesis() -> Self {
        let block = Self::new(&vec![BinaryData::new(&b"Genesis block".to_vec())],
                                  Sha256Hash::default(), 0,0);
        match block.mine(GENESIS_DIFFICULTY) {
            Some(nonce) => {
                let mut inner = block.inner.read().unwrap().clone();
                inner.nonce = nonce;
                Self {
                    inner: Arc::new(RwLock::new(inner))
                }
            }
            None => {
                panic!("Failed to mine genesis block");
            }
        }
    }

    fn next_block(&self, index: i64) -> Self {
        let inner = self.inner.read().unwrap();
        let next_block = inner.id + 1;
        Self::new(&vec![BinaryData::new(&format!("Block {}", next_block).as_bytes().to_vec())],
                  inner.hash(), index, next_block)
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
    ];
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
    fn debug(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn debug(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }

    fn as_binary_data(&self) -> Option<&BinaryData> { Some(&self) }
}

graphql_object!(BinaryData: () |&self|{
    field data() -> String {
        hex::encode(&self.data)
    }
});

#[derive(Debug)]
struct Blockchain {
    blocks: Vec<Block>,
    hash_index_map: HashMap<Sha256Hash, i64>,
    pending_data: Vec<Box<BlockData>>,
}

impl Blockchain {
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            pending_data: vec![],
            hash_index_map: HashMap::new(),
        }
    }

    fn init_genesis(&mut self) {
        println!("Initialising genesis block");
        self.blocks.push(Block::genesis());
        println!("New chain initialised");
    }

    fn add_block(&mut self) {
        let block: Block;
        match self.blocks.last() {
            Some(last_block) => {
                block = last_block.next_block((self.blocks.len()-1) as i64);
            }
            None => {
                println!("No parent");
                return;
            }
        }
        match block.mine(GENESIS_DIFFICULTY) {
            Some(nonce) => {
                let mut inner = block.inner.read().unwrap().clone();
                inner.nonce = nonce;
                self.blocks.push(Block {
                    inner: Arc::new(RwLock::new(inner))
                })
            }
            None => {
                panic!("Failed to mine block");
            }
        }
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
    let mut block_data = protobuf::RepeatedField::<chain::Block_Data>::default();
    let inner = block.inner.read().unwrap();
    for data in inner.data.iter() {
        match data.as_binary_data() {
            Some(data) => {
                let mut data_msg = chain::BinaryData::new();
                data_msg.set_data(data.data.clone());
                let mut block_data_msg = chain::Block_Data::new();
                block_data_msg.set_field_type(chain::Block_DataType::BINARY_DATA);
                block_data_msg.set_binaryData(data_msg);
                block_data.push(block_data_msg);
                continue;
            }
            None => {}
        };
        match data.as_transaction() {
            Some(data) => {
                let mut data_msg = chain::Transaction::new();
                data_msg.set_from(data.sender.to_vec());
                data_msg.set_to(data.recipient.to_vec());
                data_msg.set_amount(data.amount);
                let mut block_data_msg = chain::Block_Data::new();
                block_data_msg.set_field_type(chain::Block_DataType::TRANSACTION);
                block_data_msg.set_transaction(data_msg);
                block_data.push(block_data_msg);
                continue;
            }
            None => {}
        };
    }

    let mut block_msg = chain::Block::new();
    block_msg.set_id(inner.id);
    block_msg.set_timestamp(inner.timestamp);
    block_msg.set_nonce(inner.nonce);
    block_msg.set_prev_block_hash(inner.prev_block_hash.to_vec());
    block_msg.set_data(block_data);
    block_msg
}

fn pb_to_block(msg: &chain::Block) -> Block {
    let mut block_data: Vec<Box<BlockData>> = vec![];
    for data in msg.get_data().iter() {
        block_data.push(match data.get_field_type() {
            chain::Block_DataType::BINARY_DATA => {
                Box::new(BinaryData {
                    data: data.get_binaryData().get_data().to_vec()
                })
            }
            chain::Block_DataType::TRANSACTION => {
                Box::new(Transaction {
                    sender: sha256hash_from_slice(data.get_transaction().get_from()),
                    recipient: sha256hash_from_slice(data.get_transaction().get_to()),
                    amount: data.get_transaction().get_amount(),
                })
            }
        })
    }

    Block::restore(
        &block_data,
        sha256hash_from_slice(msg.get_prev_block_hash()),
        msg.get_id(),
        msg.get_timestamp(),
        msg.get_nonce(),
    )
}

fn write_pb_block(block: &chain::Block, hash: Sha256Hash) -> protobuf::error::ProtobufResult<()> {
    match fs::File::create(format!("./blocks/block{}", hex::encode(hash))) {
        Ok(mut out) => {
            block.write_to_writer(&mut out)
        }
        Err(e) => {
            Err(protobuf::error::ProtobufError::IoError(e))
        }
    }
}

fn read_pb_block(path: &path::Path) -> io::Result<chain::Block> {
    let mut f = fs::File::open(path)?;
    match protobuf::parse_from_reader::<chain::Block>(&mut f) {
        Ok(block) => {
            Ok(block)
        }
        Err(e) => {
            return Err(io::Error::new(io::ErrorKind::Other, e));
        }
    }
}

fn start_db_thread(blockchain: Arc<RwLock<Blockchain>>) -> (Arc<RwLock<bool>>, Arc<RwLock<bool>>) {
    let (tx, rx) = mpsc::channel();
    let exit_flag = Arc::new(RwLock::new(false));
    let exited_flag = Arc::new(RwLock::new(false));

    let exited_flag_2 = Arc::clone(&exited_flag);
    thread::spawn(move || {
        loop {
            match rx.recv() {
                Ok(_) => {
                    println!("Syncing blocks to db");
                    for block in blockchain.read().unwrap().blocks.iter() {
                        println!("{:?}", block);
                        let block_msg = block_to_pb(block);
                        match write_pb_block(&block_msg, block.hash()) {
                            Ok(_) => {}
                            Err(e) => {
                                panic!("Failed to write block to fs: {}", e)
                            }
                        }
                    }
                }
                Err(_) => {
                    *exited_flag_2.write().unwrap() = true;
                    break;
                }
            }
        }
    });

    let exit_flag_2 = Arc::clone(&exit_flag);
    thread::spawn(move|| {
        let timer = Timer::new();
        let guard = {
            let timer_tx = mpsc::Sender::clone(&tx);
            timer.schedule_repeating(chrono::Duration::seconds(5), move || {
                timer_tx.send("sync".to_string()).unwrap();
            })
        };
        loop {
            if *exit_flag_2.read().unwrap() {
                drop(guard);
                tx.send("sync".to_string()).unwrap();
                break;
            }
        }
    });

    (exit_flag, exited_flag)
}

fn find_block_files() -> io::Result<Vec<path::PathBuf>> {
    let re = Regex::new(r"^block[0-9a-fA-F]+$").unwrap();

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
                                files.push(path.clone());
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(files)
}

fn init_db() -> io::Result<Blockchain> {
    let mut chain = Blockchain::new();
    let files = find_block_files()?;

    for block in files.iter() {
        let block = pb_to_block(&read_pb_block(block)?);
        if block.is_valid(GENESIS_DIFFICULTY) {
            let hash = block.hash();
            chain.blocks.push(block);
            chain.hash_index_map.insert(hash, (chain.blocks.len()-1) as i64);
        } else {
            println!("Encountered invalid block!");
        }
    }

    for block in chain.blocks.iter() {
        let mut inner = block.inner.write().unwrap();
        match chain.hash_index_map.get(&inner.prev_block_hash) {
            Some(index) => {
                inner.prev_block_index = *index
            }
            None => {}
        }
    }

    if chain.blocks.len() == 0 {
        chain.init_genesis();
        return Ok(chain);
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
            panic!("Cannot initialise chain: {}", e);
        }
    }

    chain.write().unwrap().add_block();

    let (exit_flag, exited_flag) = start_db_thread(chain.clone());

    ctrlc::set_handler(move || {
        *exit_flag.write().unwrap() = true;
        while !*exited_flag.read().unwrap() {}
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

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
    ).run(([127, 0, 0, 1], 3000));
}