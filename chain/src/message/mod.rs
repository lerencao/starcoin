/// message for chain actor
use actix::prelude::*;
use anyhow::Result;
use crypto::HashValue;
use types::{
    block::{Block, BlockHeader, BlockTemplate},
    startup_info::ChainInfo,
    transaction::SignedUserTransaction,
};

#[derive(Clone)]
pub enum ChainRequest {
    CurrentHeader(),
    GetHeaderByHash(HashValue),
    HeadBlock(),
    GetHeaderByNumber(u64),
    GetBlockByNumber(u64),
    CreateBlockTemplate(Option<HashValue>, Vec<SignedUserTransaction>), // just fot test
    GetBlockByHash(HashValue),
    ConnectBlock(Block),
    GetHeadBranch(),
    GetChainInfo(),
    GenTx(), // just for test
}

impl Message for ChainRequest {
    type Result = Result<ChainResponse>;
}

#[derive(Clone)]
pub enum ChainResponse {
    BlockTemplate(BlockTemplate),
    Block(Block),
    OptionBlock(Option<Block>),
    BlockHeader(BlockHeader),
    HashValue(HashValue),
    ChainInfo(ChainInfo),
    None,
}
