use agent::keys::Keys;
use error::HolochainError;
// use futures::executor::block_on;
use hash_table::{pair::Pair, pair_meta::PairMeta,};
use riker::actors::*;
// use riker_default::DefaultModel;
// use riker_patterns::ask::ask;
use snowflake;
// use actor::Protocol;
use actor::SYS;
use actor::AskSelf;
use actor::Protocol;
use hash_table::HashTable;

// anything that can be asked of HashTable and block on responses
// needed to support implementing ask on upstream ActorRef from riker
pub trait AskHashTable: HashTable {}

impl AskHashTable for ActorRef<Protocol> {}

impl HashTable for ActorRef<Protocol> {
    fn setup(&mut self) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::Setup);
        unwrap_to!(response => Protocol::SetupResult).clone()
    }

    fn teardown(&mut self) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::Teardown);
        unwrap_to!(response => Protocol::TeardownResult).clone()
    }

    fn commit(&mut self, pair: &Pair) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::Commit(pair.clone()));
        unwrap_to!(response => Protocol::CommitResult).clone()
    }

    fn pair(&self, key: &str) -> Result<Option<Pair>, HolochainError> {
        let response = self.block_on_ask(Protocol::GetPair(key.to_string()));
        unwrap_to!(response => Protocol::GetPairResult).clone()
    }

    fn modify(
        &mut self,
        keys: &Keys,
        old_pair: &Pair,
        new_pair: &Pair,
    ) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::Modify {
            keys: keys.clone(),
            old_pair: old_pair.clone(),
            new_pair: new_pair.clone(),
        });
        unwrap_to!(response => Protocol::ModifyResult).clone()
    }

    fn retract(&mut self, keys: &Keys, pair: &Pair) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::Retract {
            keys: keys.clone(),
            pair: pair.clone(),
        });
        unwrap_to!(response => Protocol::RetractResult).clone()
    }

    fn assert_meta(&mut self, meta: &PairMeta) -> Result<(), HolochainError> {
        let response = self.block_on_ask(Protocol::AssertMeta(meta.clone()));
        unwrap_to!(response => Protocol::AssertMetaResult).clone()
    }

    fn get_meta(&mut self, key: &str) -> Result<Option<PairMeta>, HolochainError> {
        let response = self.block_on_ask(Protocol::GetMeta(key.to_string()));
        unwrap_to!(response => Protocol::GetMetaResult).clone()
    }

    fn get_pair_meta(&mut self, pair: &Pair) -> Result<Vec<PairMeta>, HolochainError> {
        let response = self.block_on_ask(Protocol::GetPairMeta(pair.clone()));
        unwrap_to!(response => Protocol::GetPairMetaResult).clone()
    }
}

#[derive(Clone, Debug)]
pub struct HashTableActor<HT: HashTable> {
    table: HT,
}

impl<HT: HashTable> HashTableActor<HT> {
    pub fn new(table: HT) -> HashTableActor<HT> {
        HashTableActor { table }
    }

    pub fn actor(table: HT) -> BoxActor<Protocol> {
        Box::new(HashTableActor::new(table))
    }

    pub fn props(table: HT) -> BoxActorProd<Protocol> {
        Props::new_args(Box::new(HashTableActor::actor), table)
    }

    pub fn new_ref(table: HT) -> ActorRef<Protocol> {
        SYS
            .actor_of(
                HashTableActor::props(table),
                &snowflake::ProcessUniqueId::new().to_string(),
            )
            .unwrap()
    }
}

impl<HT: HashTable> Actor for HashTableActor<HT> {
    type Msg = Protocol;

    fn receive(
        &mut self,
        context: &Context<Self::Msg>,
        message: Self::Msg,
        sender: Option<ActorRef<Self::Msg>>,
    ) {
        sender
            .try_tell(
                // deliberately exhaustively matching here, don't give into _ temptation
                match message {
                    Protocol::Setup => Protocol::SetupResult(self.table.setup()),
                    Protocol::SetupResult(_) => unreachable!(),

                    Protocol::Teardown => {
                        Protocol::TeardownResult(self.table.teardown())
                    }
                    Protocol::TeardownResult(_) => unreachable!(),

                    Protocol::Commit(pair) => {
                        Protocol::CommitResult(self.table.commit(&pair))
                    }
                    Protocol::CommitResult(_) => unreachable!(),

                    Protocol::GetPair(hash) => {
                        Protocol::GetPairResult(self.table.pair(&hash))
                    }
                    Protocol::GetPairResult(_) => unreachable!(),

                    Protocol::Modify {
                        keys,
                        old_pair,
                        new_pair,
                    } => Protocol::ModifyResult(
                        self.table.modify(&keys, &old_pair, &new_pair),
                    ),
                    Protocol::ModifyResult(_) => unreachable!(),

                    Protocol::Retract { keys, pair } => {
                        Protocol::RetractResult(self.table.retract(&keys, &pair))
                    }
                    Protocol::RetractResult(_) => unreachable!(),

                    Protocol::AssertMeta(pair_meta) => {
                        Protocol::AssertMetaResult(self.table.assert_meta(&pair_meta))
                    }
                    Protocol::AssertMetaResult(_) => unreachable!(),

                    Protocol::GetMeta(key) => {
                        Protocol::GetMetaResult(self.table.get_meta(&key))
                    }
                    Protocol::GetMetaResult(_) => unreachable!(),

                    Protocol::GetPairMeta(pair) => {
                        Protocol::GetPairMetaResult(self.table.get_pair_meta(&pair))
                    }
                    Protocol::GetPairMetaResult(_) => unreachable!(),

                    _ => unreachable!(),
                },
                Some(context.myself()),
            )
            .unwrap();
    }
}

#[cfg(test)]
pub mod tests {

    use super::HashTableActor;
    use hash_table::{memory::tests::test_table};
    use riker::actors::*;
    use hash::tests::test_hash;
    // use hash_table::HashTable;
    use hash_table::pair::tests::test_pair;
    use std::thread;
    use std::sync::mpsc;
    use actor::Protocol;
    use hash_table::HashTable;

    /// dummy table actor ref
    /// every call produces a new actor, not just a new ref to the same actor
    pub fn test_table_actor() -> ActorRef<Protocol> {
        HashTableActor::new_ref(test_table())
    }

    #[test]
    fn round_trip() {
        let mut table_actor = test_table_actor();

        assert_eq!(
            table_actor.pair(&test_hash()).unwrap(),
            None,
        );

        table_actor.commit(&test_pair()).unwrap();

        assert_eq!(
            table_actor.pair(&test_pair().key()).unwrap(),
            Some(test_pair()),
        );
    }

    #[test]
    /// show two things here:
    /// - we can clone some stateful thing (i.e. actor ref) and mutate one clone and have that
    ///   consistent across all the clones
    /// - we can send the cloned stateful thing into threads and have them see a consistent world
    ///   view without juggling direct message passing through channels
    fn test_round_trip_threads() {
        let table_actor = test_table_actor();

        let table_actor_thread = table_actor.clone();
        let (tx1, rx1) = mpsc::channel();
        thread::spawn(move || {
            assert_eq!(
                table_actor_thread.pair(&test_hash()).unwrap(),
                None,
            );
            // kick off the next thread
            tx1.send(true).unwrap();
        });

        // mutate this clone of the original actor ref
        let mut table_actor_thread = table_actor.clone();
        let (tx2, rx2) = mpsc::channel();
        thread::spawn(move || {
            rx1.recv().unwrap();
            let pair = test_pair();
            table_actor_thread.commit(&pair).unwrap();
            // push the committed pair through to the next thread
            tx2.send(pair).unwrap();
        });

        let table_actor_thread = table_actor.clone();
        let handle = thread::spawn(move || {
            let pair = rx2.recv().unwrap();
            assert_eq!(
                table_actor_thread.pair(&pair.key()).unwrap(),
                Some(pair),
            );
        });

        handle.join().unwrap();
    }

}
