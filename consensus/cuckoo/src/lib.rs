pub mod util;
pub mod consensus;
use crate::util::blake2b_256;
use byteorder::{ByteOrder, LittleEndian};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Cuckoo {
    pub max_edge: u64,
    pub edge_mask: u64,
    pub cycle_length: usize,
}

impl Cuckoo {
    pub fn new(edge_bits: u8, cycle_length: usize) -> Self {
        assert!(cycle_length > 0, "cycle_length must be larger than 0");
        Self {
            max_edge: 1 << edge_bits,
            edge_mask: (1 << edge_bits) - 1,
            cycle_length,
        }
    }
    pub fn verify(&self, input: &[u8], proof: &[u32]) -> bool {
        if proof.len() != self.cycle_length {
            return false;
        }

        if u64::from(proof[self.cycle_length - 1]) > self.edge_mask {
            return false;
        }

        let is_monotonous = proof.windows(2).all(|w| w[0] < w[1]);
        if !is_monotonous {
            return false;
        }

        let keys = CuckooSip::input_to_keys(&input);
        let hasher = CuckooSip::new(keys[0], keys[1], keys[2], keys[3]);

        let mut from_upper: HashMap<_, Vec<_>> = HashMap::with_capacity(proof.len());
        let mut from_lower: HashMap<_, Vec<_>> = HashMap::with_capacity(proof.len());
        for (u, v) in proof.iter().map(|i| hasher.edge(*i, self.edge_mask)) {
            from_upper
                .entry(u)
                .and_modify(|upper| upper.push(v))
                .or_insert_with(|| vec![v]);
            from_lower
                .entry(v)
                .and_modify(|lower| lower.push(u))
                .or_insert_with(|| vec![u]);
        }
        if from_upper.values().any(|list| list.len() != 2) {
            return false;
        }
        if from_lower.values().any(|list| list.len() != 2) {
            return false;
        }

        let mut cycle_length = 0;
        let mut cur_edge = hasher.edge(proof[0], self.edge_mask);
        let start = cur_edge.0;
        loop {
            let next_lower = *from_upper[&cur_edge.0]
                .iter()
                .find(|v| **v != cur_edge.1)
                .expect("next_lower should be found");
            let next_upper = *from_lower[&next_lower]
                .iter()
                .find(|u| **u != cur_edge.0)
                .expect("next_upper should be found");
            cur_edge = (next_upper, next_lower);
            cycle_length += 2;

            if start == cur_edge.0 {
                break;
            }
        }
        cycle_length == self.cycle_length
    }
}

pub struct CuckooSip {
    keys: [u64; 4],
}

impl CuckooSip {
    pub fn new(key0: u64, key1: u64, key2: u64, key3: u64) -> Self {
        Self {
            keys: [key0, key1, key2, key3],
        }
    }

    fn hash(&self, val: u64) -> u64 {
        let mut v0 = self.keys[0];
        let mut v1 = self.keys[1];
        let mut v2 = self.keys[2];
        let mut v3 = self.keys[3] ^ val;
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        v0 ^= val;
        v2 ^= 0xff;
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);
        CuckooSip::sipround(&mut v0, &mut v1, &mut v2, &mut v3);

        v0 ^ v1 ^ v2 ^ v3
    }

    fn sipround(v0: &mut u64, v1: &mut u64, v2: &mut u64, v3: &mut u64) {
        *v0 = v0.wrapping_add(*v1);
        *v2 = v2.wrapping_add(*v3);
        *v1 = v1.rotate_left(13);

        *v3 = v3.rotate_left(16);
        *v1 ^= *v0;
        *v3 ^= *v2;

        *v0 = v0.rotate_left(32);
        *v2 = v2.wrapping_add(*v1);
        *v0 = v0.wrapping_add(*v3);

        *v1 = v1.rotate_left(17);
        *v3 = v3.rotate_left(21);

        *v1 ^= *v2;
        *v3 ^= *v0;
        *v2 = v2.rotate_left(32);
    }

    pub fn edge(&self, val: u32, edge_mask: u64) -> (u64, u64) {
        let upper = self.hash(u64::from(val) << 1) & edge_mask;
        let lower = self.hash((u64::from(val) << 1) + 1) & edge_mask;

        (upper, lower)
    }

    pub fn input_to_keys(input: &[u8]) -> [u64; 4] {
        let result = blake2b_256(input.as_ref());
        [
            LittleEndian::read_u64(&result[0..8]).to_le(),
            LittleEndian::read_u64(&result[8..16]).to_le(),
            LittleEndian::read_u64(&result[16..24]).to_le(),
            LittleEndian::read_u64(&result[24..32]).to_le(),
        ]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::blake2b_256;

    #[test]
    fn test_pow() {
        let input =
            [238, 237, 143, 251, 211, 26, 16, 237, 158, 89, 77, 62, 49, 241, 85, 233, 49, 77,
                230, 148, 177, 49, 129, 38, 152, 148, 40, 170, 1, 115, 145, 191, 44, 10, 206, 23,
                226, 132, 186, 196, 204, 205, 133, 173, 209, 20, 116, 16, 159, 161, 117, 167, 151,
                171, 246, 181, 209, 140, 189, 163, 206, 155, 209, 157, 110, 2, 79, 249, 34, 228,
                252, 245, 141, 27, 9, 156, 85, 58, 121, 46];

        let proof = [1, 12, 23, 27, 31, 48, 50, 60];
        let pow = &Cuckoo::new(6, 8);
        assert!(pow.verify(&input, &proof));
    }
}
