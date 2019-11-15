use grpcio;
use std::{sync::Arc, pin::Pin, thread};
use grpcio::{ChannelBuilder, EnvBuilder};
use proto::miner::{MineCtxRequest, MinedBlockRequest, MinerProxyClient, MineCtx as MineCtxRpc};
use async_std::{
    task,
    stream::Stream,
    prelude::*,
    task::{Context, Poll},
};
use miner::types::{MineCtx, MAX_EDGE, CYCLE_LENGTH};
use std::{task::Waker, sync::Mutex};
use byteorder::{ByteOrder, LittleEndian};
use cuckoo::util::blake2b_256;

struct MineCtxStream {
    client: MinerProxyClient,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl MineCtxStream {
    fn new(client: MinerProxyClient) -> Self {
        let waker: Arc<Mutex<Option<Waker>>> = Arc::new(Mutex::new(None));
        let task_waker = waker.clone();

        task::spawn(async move {
            loop {
                thread::sleep(std::time::Duration::from_secs(1));
                let mut inner_waker = task_waker.lock().unwrap();
                if let Some(waker) = inner_waker.take() {
                    waker.wake();
                }
            }
        });
        MineCtxStream {
            client,
            waker,
        }
    }
}

impl Stream for MineCtxStream {
    type Item = MineCtx;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut waker = self.waker.lock().unwrap();
        match self.client.get_mine_ctx(&MineCtxRequest {}) {
            Ok(resp) => {
                if let Some(mine_ctx) = resp.mine_ctx {
                    let ctx = MineCtx { header: mine_ctx.header, nonce: mine_ctx.nonce };
                    Poll::Ready(Some(ctx))
                } else {
                    *waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
            Err(_e) => {
                *waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

struct MineClient {
    rpc_client: MinerProxyClient
}

impl MineClient {
    pub fn new(miner_server: String) -> Self {
        let env = Arc::new(EnvBuilder::new().build());
        let ch = ChannelBuilder::new(env).connect(&miner_server);
        let rpc_client = MinerProxyClient::new(ch);
        MineClient {
            rpc_client
        }
    }

    pub async fn start(&self) {
        let mut ctx_stream = MineCtxStream::new(self.rpc_client.clone());
        while let Some(ctx) = ctx_stream.next().await {
            println!("the ctx is {:?}", ctx);
            let proof = mine(&ctx.header, ctx.nonce, MAX_EDGE, CYCLE_LENGTH);
            if let Some(proof) = proof {
                let req = MinedBlockRequest {
                    mine_ctx: Some(MineCtxRpc {
                        header: ctx.header,
                        nonce: ctx.nonce,
                    }),
                    proof,
                };
                let resp = self.rpc_client.mined(&req);
                println!("mined{:?}", resp);
            } else {
                thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
}

extern "C" {
    pub fn c_solve(output: *mut u32, input: *const u8, max_edge: u64, cycle_length: u32) -> u32;
}

fn pow_input(header_hash: &[u8], nonce: u64) -> [u8; 40] {
    let mut input = [0; 40];
    assert!(header_hash.len() == 32);
    input[8..40].copy_from_slice(&header_hash[..32]);
    LittleEndian::write_u64(&mut input, nonce);
    input
}

pub fn mine(header_hash: &[u8], nonce: u64, max_edge_bits: u8, cycle_length: usize) -> Option<Vec<u8>> {
    unsafe {
        let pow_input = pow_input(header_hash, nonce);
        let input = blake2b_256(&pow_input.as_ref());
        let mut output = vec![0u32; cycle_length];
        let max_edge = 1 << max_edge_bits;
        println!("what");
        if c_solve(
            output.as_mut_ptr(),
            input.as_ptr(),
            max_edge,
            cycle_length as u32,
        ) > 0
        {
            let mut output_u8 = vec![0u8; CYCLE_LENGTH << 2];
            LittleEndian::write_u32_into(&output, &mut output_u8);
            return Some(output_u8);
        }
        return None;
    }
}

fn main() {
    let miner = MineClient::new("127.0.0.1:4251".to_string());
    task::block_on(
        miner.start()
    );
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mine() {
        unsafe {
            let mut output = vec![0u32; CYCLE_LENGTH];
            let input = [238, 237, 143, 251, 211, 26, 16, 237, 158, 89, 77, 62, 49, 241, 85, 233, 49, 77,
                230, 148, 177, 49, 129, 38, 152, 148, 40, 170, 1, 115, 145, 191, 44, 10, 206, 23,
                226, 132, 186, 196, 204, 205, 133, 173, 209, 20, 116, 16, 159, 161, 117, 167, 151,
                171, 246, 181, 209, 140, 189, 163, 206, 155, 209, 157, 110, 2, 79, 249, 34, 228,
                252, 245, 141, 27, 9, 156, 85, 58, 121, 46];
            let input = blake2b_256(input.as_ref());
            if c_solve(
                output.as_mut_ptr(),
                input.as_ptr(),
                1 << MAX_EDGE,
                CYCLE_LENGTH as u32,
            ) > 0
            {
                let mut output_u8 = vec![0u8; CYCLE_LENGTH << 2];
                LittleEndian::write_u32_into(&output, &mut output_u8);
                return;
            }
            panic!("test failed");
        }
    }
}