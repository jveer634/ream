use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Beacon State table
///
/// Key: block_root
/// Value: BeaconState
pub const BEACON_STATE_TABLE: TableDefinition<SSZEncoding<B256>, SSZEncoding<BeaconState>> =
    TableDefinition::new("beacon_state");

pub struct BeaconStateTable {
    pub db: Arc<Database>,
}

impl Table for BeaconStateTable {
    type Key = B256;

    type Value = BeaconState;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(BEACON_STATE_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(BEACON_STATE_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
