// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod consensusdb_test;
mod schema;

use crate::chained_bft::consensusdb::schema::{
    block::{BlockSchema, SchemaBlock},
    block_index::BlockIndexSchema,
    quorum_certificate::QCSchema,
    single_entry::{SingleEntryKey, SingleEntrySchema},
};
use anyhow::{ensure, Result};
use consensus_types::{block::Block, common::Payload, quorum_cert::QuorumCert};
use libra_crypto::HashValue;
use libra_logger::prelude::*;
use libra_types::block_index::BlockIndex;
use schema::{BLOCK_CF_NAME, BLOCK_INDEX_CF_NAME, QC_CF_NAME, SINGLE_ENTRY_CF_NAME};
use schemadb::{
    ColumnFamilyOptions, ColumnFamilyOptionsMap, ReadOptions, SchemaBatch, DB, DEFAULT_CF_NAME,
};
use std::{collections::HashMap, iter::Iterator, path::Path, time::Instant};

type HighestTimeoutCertificate = Vec<u8>;
type VoteMsgData = Vec<u8>;

pub struct ConsensusDB {
    db: DB,
}

impl ConsensusDB {
    pub fn new<P: AsRef<Path> + Clone>(db_root_path: P) -> Self {
        let cf_opts_map: ColumnFamilyOptionsMap = [
            (
                /* UNUSED CF = */ DEFAULT_CF_NAME,
                ColumnFamilyOptions::default(),
            ),
            (BLOCK_CF_NAME, ColumnFamilyOptions::default()),
            (QC_CF_NAME, ColumnFamilyOptions::default()),
            (SINGLE_ENTRY_CF_NAME, ColumnFamilyOptions::default()),
            (BLOCK_INDEX_CF_NAME, ColumnFamilyOptions::default()),
        ]
        .iter()
        .cloned()
        .collect();

        let path = db_root_path.as_ref().join("consensusdb");
        let instant = Instant::now();
        let db = DB::open(path.clone(), cf_opts_map)
            .expect("ConsensusDB open failed; unable to continue");

        info!(
            "Opened ConsensusDB at {:?} in {} ms",
            path,
            instant.elapsed().as_millis()
        );

        Self { db }
    }

    pub fn get_data<T: Payload>(
        &self,
    ) -> Result<(
        Option<VoteMsgData>,
        Option<HighestTimeoutCertificate>,
        Vec<Block<T>>,
        Vec<QuorumCert>,
    )> {
        let last_vote_msg_data = self.get_last_vote_msg_data()?;
        let highest_timeout_certificate = self.get_highest_timeout_certificate()?;
        let consensus_blocks = self
            .get_blocks()?
            .into_iter()
            .map(|(_block_hash, block_content)| block_content)
            .collect::<Vec<_>>();
        let consensus_qcs = self
            .get_quorum_certificates()?
            .into_iter()
            .map(|(_block_hash, qc)| qc)
            .collect::<Vec<_>>();
        Ok((
            last_vote_msg_data,
            highest_timeout_certificate,
            consensus_blocks,
            consensus_qcs,
        ))
    }

    pub fn save_highest_timeout_certificate(
        &self,
        highest_timeout_certificate: HighestTimeoutCertificate,
    ) -> Result<()> {
        let mut batch = SchemaBatch::new();
        batch.put::<SingleEntrySchema>(
            &SingleEntryKey::HighestTimeoutCertificate,
            &highest_timeout_certificate,
        )?;
        self.commit(batch)
    }

    pub fn save_state(&self, last_vote: VoteMsgData) -> Result<()> {
        let mut batch = SchemaBatch::new();
        batch.put::<SingleEntrySchema>(&SingleEntryKey::LastVoteMsg, &last_vote)?;
        self.commit(batch)
    }

    pub fn save_blocks_and_quorum_certificates<T: Payload>(
        &self,
        block_data: Vec<Block<T>>,
        qc_data: Vec<QuorumCert>,
    ) -> Result<()> {
        ensure!(
            !block_data.is_empty() || !qc_data.is_empty(),
            "Consensus block and qc data is empty!"
        );
        let mut batch = SchemaBatch::new();
        block_data
            .iter()
            .map(|block| {
                batch.put::<BlockSchema<T>>(
                    &block.id(),
                    &SchemaBlock::<T>::from_block(block.clone()),
                )
            })
            .collect::<Result<()>>()?;
        qc_data
            .iter()
            .map(|qc| batch.put::<QCSchema>(&qc.certified_block().id(), qc))
            .collect::<Result<()>>()?;
        self.commit(batch)
    }

    pub fn delete_blocks_and_quorum_certificates<T: Payload>(
        &self,
        block_ids: Vec<HashValue>,
    ) -> Result<()> {
        ensure!(!block_ids.is_empty(), "Consensus block ids is empty!");
        let mut batch = SchemaBatch::new();
        block_ids
            .iter()
            .map(|hash| {
                batch.delete::<BlockSchema<T>>(hash)?;
                batch.delete::<QCSchema>(hash)
            })
            .collect::<Result<_>>()?;
        self.commit(batch)
    }

    /// Write the whole schema batch including all data necessary to mutate the ledger
    /// state of some transaction by leveraging rocksdb atomicity support.
    fn commit(&self, batch: SchemaBatch) -> Result<()> {
        self.db.write_schemas(batch)
    }

    /// Get latest timeout certificates (we only store the latest highest timeout certificates).
    fn get_highest_timeout_certificate(&self) -> Result<Option<Vec<u8>>> {
        self.db
            .get::<SingleEntrySchema>(&SingleEntryKey::HighestTimeoutCertificate)
    }

    /// Delete the timeout certificates
    pub fn delete_highest_timeout_certificate(&self) -> Result<()> {
        let mut batch = SchemaBatch::new();
        batch.delete::<SingleEntrySchema>(&SingleEntryKey::HighestTimeoutCertificate)?;
        self.commit(batch)
    }

    /// Get latest vote message data (if available)
    fn get_last_vote_msg_data(&self) -> Result<Option<Vec<u8>>> {
        self.db
            .get::<SingleEntrySchema>(&SingleEntryKey::LastVoteMsg)
    }

    pub fn delete_last_vote_msg(&self) -> Result<()> {
        let mut batch = SchemaBatch::new();
        batch.delete::<SingleEntrySchema>(&SingleEntryKey::LastVoteMsg)?;
        self.commit(batch)
    }

    /// Get all consensus blocks.
    fn get_blocks<T: Payload>(&self) -> Result<HashMap<HashValue, Block<T>>> {
        let mut iter = self.db.iter::<BlockSchema<T>>(ReadOptions::default())?;
        iter.seek_to_first();
        iter.map(|value| value.and_then(|(k, v)| Ok((k, v.borrow_into_block().clone()))))
            .collect::<Result<HashMap<HashValue, Block<T>>>>()
    }

    /// Get all consensus QCs.
    fn get_quorum_certificates(&self) -> Result<HashMap<HashValue, QuorumCert>> {
        let mut iter = self.db.iter::<QCSchema>(ReadOptions::default())?;
        iter.seek_to_first();
        iter.collect::<Result<HashMap<HashValue, QuorumCert>>>()
    }

    /// Get block by hash
    pub fn get_block_by_hash<T: Payload>(&self, hash: &HashValue) -> Option<Block<T>> {
        match self.db.get::<BlockSchema<T>>(&hash) {
            Ok(block) => match block {
                Some(b) => Some(b.borrow_into_block().clone()),
                None => None,
            },
            _ => None,
        }
    }

    /// Get blocks by hashs
    pub fn get_blocks_by_hashs<T: Payload>(&self, hashs: Vec<HashValue>) -> Option<Vec<Block<T>>> {
        let mut blocks = vec![];
        for hash in hashs {
            match self.get_block_by_hash(&hash) {
                Some(b) => blocks.push(b),
                None => return None,
            }
        }

        return Some(blocks);
    }

    /// Insert BlockIndex
    pub fn insert_block_index(&self, height: &u64, block_index: &BlockIndex) -> Result<()> {
        let mut batch = SchemaBatch::new();
        batch.put::<BlockIndexSchema>(&height, &block_index)?;
        self.commit(batch)
    }

    /// Load BlockIndex
    pub fn _load_block_index(&self) -> Result<Vec<BlockIndex>> {
        unimplemented!()
    }

    pub fn query_block_index(&self, height: Option<u64>, size: usize) -> Result<Vec<BlockIndex>> {
        let mut block_index_list = vec![];
        let mut begin = match height {
            Some(h) => {
                if h > 0 {
                    h - 1
                } else {
                    0
                }
            }
            None => {
                let mut iter = self.db.iter::<BlockIndexSchema>(ReadOptions::default())?;
                iter.seek_to_last();
                let result = iter.next();
                match result {
                    Some(val) => {
                        let (k, _v) = val.expect("Get value from db err.");
                        k
                    }
                    None => 0, //todo:err
                }
            }
        };

        while block_index_list.len() < size {
            let block_index: Option<BlockIndex> = self.db.get::<BlockIndexSchema>(&begin)?;
            match block_index {
                Some(index) => {
                    block_index_list.push(index);
                    begin = begin - 1;
                }
                None => {
                    break;
                }
            }
        }
        block_index_list.reverse();
        Ok(block_index_list)
    }

    pub fn query_blocks_by_height<T: Payload>(&self, begin_height: u64, size: usize) -> Result<Vec<Block<T>>> {
        let mut block_list = vec![];
        let mut height = begin_height;

        while block_list.len() < size {
            let block_index: Option<BlockIndex> = self.db.get::<BlockIndexSchema>(&height)?;
            match block_index {
                Some(index) => {
                    let block = self.get_block_by_hash(&index.id());
                    match block {
                        Some(b) => {
                            block_list.push(b);
                            height = height + 1;
                        },
                        None => break,
                    }
                }
                None => break,
            }
        }
        Ok(block_list)
    }

    pub fn query_block_index_by_height(&self, height: u64) -> Result<Option<BlockIndex>> {
        let block_index: Option<BlockIndex> = self.db.get::<BlockIndexSchema>(&height)?;
        Ok(block_index)
    }

    pub fn latest_block_index(&self) -> Option<(u64, BlockIndex)> {
        let mut iter = self
            .db
            .iter::<BlockIndexSchema>(ReadOptions::default())
            .expect("db err.");
        iter.seek_to_last();
        let result = iter.next();
        match result {
            Some(val) => {
                let (k, v) = val.expect("Get value from db err.");
                Some((k, v))
            }
            None => None,
        }
    }

    pub fn latest_height(&self) -> Option<u64> {
        let latest_block_index = self.latest_block_index();
        match latest_block_index {
            Some((height, _block_index)) => return Some(height),
            None => return None,
        }
    }

    pub fn latest_block<T: Payload>(&self) -> Option<Block<T>> {
        let latest_block_index = self.latest_block_index();
        match latest_block_index {
            Some((_height, block_index)) => {
                let block_id = block_index.id();
                self.get_block_by_hash(&block_id)
            }
            None => return None,
        }
    }
}
