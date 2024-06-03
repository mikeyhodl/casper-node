use crate::{
    abi::{CasperABI, Declaration, Definition, StructField},
    host::{self, read_vec},
};

use crate::serializers::borsh::{self, BorshDeserialize, BorshSerialize};
use const_fnv1a_hash::fnv1a_hash_str_64;
use vm_common::keyspace::Keyspace;

use std::marker::PhantomData;

#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
#[borsh(crate = "crate::serializers::borsh")]
pub struct Map<K, V> {
    pub(crate) name: String,
    pub(crate) _marker: PhantomData<(K, V)>,
}

/// Computes the prefix for a given key.
#[allow(dead_code)]
pub(crate) const fn compute_prefix(input: &str) -> [u8; 8] {
    let hash = fnv1a_hash_str_64(input);
    hash.to_le_bytes()
}

impl<K, V> Map<K, V>
where
    K: BorshSerialize,
    V: BorshSerialize + BorshDeserialize,
{
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            _marker: PhantomData,
        }
    }

    pub fn insert(&mut self, key: &K, value: &V) {
        let mut context_key = Vec::new();
        context_key.extend(self.name.as_bytes());
        // NOTE: We may want to create new keyspace for a hashed context element to avoid hashing in
        // the wasm.
        key.serialize(&mut context_key).unwrap();
        let prefix = Keyspace::Context(&context_key);
        host::casper_write(prefix, &borsh::to_vec(value).unwrap()).unwrap();
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut key_bytes = self.name.as_bytes().to_owned();
        key.serialize(&mut key_bytes).unwrap();
        let prefix = Keyspace::Context(&key_bytes);
        match read_vec(prefix) {
            Some(vec) => Some(borsh::from_slice(&vec).unwrap()),
            None => None,
        }
    }
}

impl<K: CasperABI, V: CasperABI> CasperABI for Map<K, V> {
    fn populate_definitions(definitions: &mut crate::abi::Definitions) {
        definitions.populate_one::<K>();
        definitions.populate_one::<V>();
    }

    fn declaration() -> Declaration {
        format!("Map<{}, {}>", K::declaration(), V::declaration())
    }
    #[inline]
    fn definition() -> Definition {
        Definition::Struct {
            items: vec![StructField {
                name: "prefix".into(),
                decl: u64::declaration(),
            }],
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn test_compute_prefix() {
        let prefix = compute_prefix("hello");
        assert_eq!(prefix.as_slice(), &[11, 189, 170, 128, 70, 216, 48, 164]);
        let back = u64::from_le_bytes(prefix);
        assert_eq!(fnv1a_hash_str_64("hello"), back);
    }

    #[test]
    fn test_map() {
        let mut map = Map::<u64, u64>::new("test");
        map.insert(&1, &2);
        assert_eq!(map.get(&1), Some(2));
        assert_eq!(map.get(&2), None);
        map.insert(&2, &3);
        assert_eq!(map.get(&1), Some(2));
        assert_eq!(map.get(&2), Some(3));

        let mut map = Map::<u64, u64>::new("test2");
        assert_eq!(map.get(&1), None);
        map.insert(&1, &22);
        assert_eq!(map.get(&1), Some(22));
        assert_eq!(map.get(&2), None);
        map.insert(&2, &33);
        assert_eq!(map.get(&1), Some(22));
        assert_eq!(map.get(&2), Some(33));
    }
}
