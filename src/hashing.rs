// #![warn(missing_docs)]

use color_eyre::eyre::{eyre, Result};
use md5::Md5;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

// set the chunk size to be 1 megabyte
const CHUNK_SIZE: usize = 1024 * 1024;

// defining the hash sizes for the two supported algorithms
const MD5_SIZE: usize = 16;
const SHA_SIZE: usize = 32;

pub enum HashingAlgo {
    Md5,
    SHA2,
}

pub struct HashRequest<'a, const N: usize> {
    pub path: &'a Path,
    pub algorithm: HashingAlgo,
}

impl<'a, const N: usize> HashRequest<'a, N> {
    pub fn new(path: &'a Path, algorithm: HashingAlgo) -> HashRequest<N> {
        HashRequest { path, algorithm }
    }

    pub fn compute_starter_hashes<D: Digest + Default>(&self) -> Result<impl MerkleIter<N>> {
        let mut hashes: Vec<[u8; N]> = Vec::new();

        let file = File::open(self.path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; CHUNK_SIZE];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            let mut hasher = D::default();
            hasher.update(&buffer[..bytes_read]);
            let hash_result: [u8; N] = hasher.finalize().as_ref().try_into()?;
            hashes.push(hash_result);
        }

        let hash_array = HashArray { hashes };
        Ok(hash_array)
    }
}

pub struct HashArray<const N: usize> {
    pub hashes: Vec<[u8; N]>,
}

pub trait MerkleIter<const N: usize> {
    fn par_iter_merkle<D: Digest + Default>(self) -> Result<HashArray<N>>;
}

impl<const N: usize> MerkleIter<N> for HashArray<N> {
    fn par_iter_merkle<D: Digest + Default>(self) -> Result<HashArray<N>> {
        if self.hashes.len() == 1 {
            return Ok(self);
        }
        let chunks = self.hashes.chunks(2).collect::<Vec<&[[u8; N]]>>();
        let current_hashes: Vec<[u8; N]> = chunks
            .into_par_iter()
            .map(|hash_pair| {
                let mut digest = D::default();
                if hash_pair.len() == 2 {
                    digest.update(hash_pair[0]);
                    digest.update(hash_pair[1]);
                } else {
                    digest.update(hash_pair[0]);
                }
                let updated_hash: [u8; N] = digest.finalize().as_ref().try_into().unwrap();
                updated_hash
            })
            .collect();

        let current_array = HashArray {
            hashes: current_hashes,
        };

        // recursively continue the search for the root hash
        let output_hashes = HashArray::par_iter_merkle::<D>(current_array)?;

        Ok(output_hashes)
    }
}

pub fn find_root_hash(input_file: &Path, algorithm: HashingAlgo) -> Result<String> {
    let root_hash_vec: Vec<Vec<u8>> = match algorithm {
        HashingAlgo::Md5 => HashRequest::new(input_file, HashingAlgo::Md5)
            .compute_starter_hashes::<Md5>()?
            .par_iter_merkle::<Md5>()?
            .hashes
            .into_iter()
            .map(|hash: [u8; MD5_SIZE]| hash.to_vec())
            .collect(),
        HashingAlgo::SHA2 => HashRequest::new(input_file, HashingAlgo::Md5)
            .compute_starter_hashes::<Sha256>()?
            .par_iter_merkle::<Sha256>()?
            .hashes
            .into_iter()
            .map(|hash: [u8; SHA_SIZE]| hash.to_vec())
            .collect(),
    };

    match std::str::from_utf8(&root_hash_vec[0]) {
        Ok(root_hash) => Ok(String::from(root_hash)),
        Err(hash_error) => Err(eyre!(
            "Error encountered while computing Merkle tree root hash for {:?}:\n{}",
            input_file,
            hash_error
        )),
    }
}
