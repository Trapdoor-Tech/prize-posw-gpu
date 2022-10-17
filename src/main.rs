// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use core::sync::atomic::{AtomicBool,Ordering};
use std::sync::atomic::AtomicU16;
use std::ops::{Add};
use std::time::{Duration, Instant};
use std::sync::mpsc;
use std::sync::Arc;
use snarkvm_dpc::{posw::PoSWCircuit, testnet2::Testnet2, BlockTemplate, Network, PoSWScheme};
use snarkvm_utilities::Uniform;
use log::info;
use rand::thread_rng;
use std::thread;
use snarkvm::utilities::sync::atomic::AtomicI64;
/// Run the PoSW prover for 20 seconds, attempting to generate as many proofs as possible.
fn main() {
    println!("Running initial setup...");

    // Construct the block template.
    let block = Testnet2::genesis_block();
    let block_template = BlockTemplate::new(
        block.previous_block_hash(),
        block.height(),
        block.timestamp(),
        block.difficulty_target(),
        block.cumulative_weight(),
        block.previous_ledger_root(),
        block.transactions().clone(),
        block
            .to_coinbase_transaction()
            .unwrap()
            .to_records()
            .next()
            .unwrap(),
    );


    let worker_num = match std::env::var("WORKER_NUM") {
        Ok(num) => match num.parse::<usize>() {
            Ok(num) => num,
            Err(e) => {
                info!("Parse worker number error {:?}, use default 1", e);
                100usize
            }
        },
        Err(e) => {
            info!("Get worker number error {:?}, use default 1", e);
            100usize
        }
    };
    const MAXIMUM_MINING_DURATION: u64 = 20;

    // Instantiate the circuit.
    let mut circuit =
        PoSWCircuit::<Testnet2>::new(&block_template, Uniform::rand(&mut thread_rng())).unwrap();

    //warm gpu
    //The GPU needs to be warmed up for startup, so the following process is the warm-up process
    thread::scope(|s| {
        let terminator = Arc::new(AtomicBool::new(false));
        for worker in 0..100 {
            let terminator_vec = terminator.clone();
            let mut circuit_warm = circuit.clone();
            s.spawn(move || {
                match Testnet2::posw()
                    .prove_once_unchecked(&mut circuit_warm, &terminator_vec, &mut thread_rng()) {
                    Ok(_proof1) => {}
                    Err(_e) => {}
                };
            });
        }
    });
    // warm gpu end
    println!("Done! Running proving challenge...");
    let now = Instant::now();
    let (tx,rx) = mpsc::channel();
    let terminator = Arc::new(AtomicBool::new(false));
    for worker in 0..worker_num {
        let thread_tx = tx.clone();
        let mut circuit = circuit.clone();
        let block_template = block_template.clone();
        let terminator_vec = terminator.clone();
        thread::spawn(move || {
            let mut proofs_generated = 0;
            loop {
                // Run one iteration of PoSW.
                // Break if time has elapsed
                if now.elapsed() > Duration::from_secs(MAXIMUM_MINING_DURATION) {
                    break;
                }
                let proof = match Testnet2::posw()
                    .prove_once_unchecked(&mut circuit, &terminator_vec, &mut thread_rng()) {
                    Ok(proof1) => proof1,
                    Err(e) => break,
                };

                // Check if the updated proof is valid.
                if !Testnet2::posw().verify(
                    block_template.difficulty_target(),
                    &circuit.to_public_inputs(),
                    &proof,
                ) {
                    panic!("proof verification failed, contestant disqualified");
                }
                // proofs_generated.fetch_add(1, Ordering::SeqCst);
                proofs_generated += 1;
            }
            thread_tx.send(proofs_generated);
        });
    }

    thread::sleep(Duration::from_secs(MAXIMUM_MINING_DURATION));
    // Break if time has elapsed
    terminator.store(true, Ordering::SeqCst);

    let mut total_size = 0 ;
    for _ in 0..worker_num{
        total_size +=rx.recv().unwrap();
    }

    println!("Finished!");
    println!("{total_size} proofs generated.");
    println!("tps is :{}", total_size as f64 / MAXIMUM_MINING_DURATION as f64);
}




